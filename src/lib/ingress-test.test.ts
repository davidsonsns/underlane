import { describe, expect, it, vi } from "vitest";
import { ingressTester, sameIngressRules, type IngressTarget } from "./ingress-test";
import type { IngressMatch, RouteMapping } from "./types";

const rule = (hostname: string | null, service = "http://localhost:3000", path: string | null = null): RouteMapping =>
  ({ hostname, path, service, inspected: false, note: null });
const baseline: IngressTarget = { id: "one", source_path: "/tmp/config.yml", run: 1, config_revision: 1 };
const result = (): IngressMatch => ({ source_path: baseline.source_path, rule_index: 0, rules: [rule("app.example.com"), rule(null, "http_status:404")], raw: "Matched rule #0" });
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function setup() {
  let target: IngressTarget | null = { ...baseline };
  const load = vi.fn<(id: string, url: string, requestId: string) => Promise<IngressMatch>>();
  const stop = vi.fn<(requestId: string) => Promise<void>>().mockResolvedValue();
  const apply = vi.fn();
  const fail = vi.fn();
  const tester = ingressTester(() => target, load, stop, apply, fail);
  return { tester, load, stop, apply, fail, select: (next: IngressTarget | null) => { target = next; } };
}

describe("offline ingress requests", () => {
  it("sends the submitted URL only to the ID-scoped tester and accepts an ordered rule result", async () => {
    const { tester, load, stop, apply, fail } = setup();
    load.mockResolvedValue(result());
    await tester.test("https://app.example.com/path?token=synthetic");
    expect(load).toHaveBeenCalledExactlyOnceWith("one", "https://app.example.com/path?token=synthetic", expect.any(String));
    expect(apply).toHaveBeenCalledExactlyOnceWith(result());
    expect(stop).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
  });
  it.each([
    ["selection", { ...baseline, id: "two" }],
    ["path", { ...baseline, source_path: "/tmp/new.yml" }],
    ["run", { ...baseline, run: 2 }],
    ["loaded config", { ...baseline, config_revision: 2 }],
    ["unsupported tunnel", null],
  ] as const)("drops pending replies when %s changes before the next render", async (_, next) => {
    const { tester, load, apply, fail, select } = setup();
    const read = deferred<IngressMatch>();
    load.mockReturnValue(read.promise);
    const pending = tester.test("https://app.example.com");
    select(next);
    read.resolve(result());
    await pending;
    expect(apply).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
  });
  it("cancels the old child on replacement and ignores its later match", async () => {
    const { tester, load, stop, apply } = setup();
    const read = deferred<IngressMatch>();
    load.mockReturnValueOnce(read.promise).mockResolvedValueOnce({ ...result(), rule_index: 1 });
    const old = tester.test("https://app.example.com");
    const oldId = load.mock.calls[0][2];
    await tester.test("https://other.example.com");
    expect(stop).toHaveBeenCalledExactlyOnceWith(oldId);
    expect(load.mock.calls[1][2]).not.toBe(oldId);
    read.resolve(result());
    await old;
    expect(apply).toHaveBeenCalledExactlyOnceWith({ ...result(), rule_index: 1 });
  });
  it("discards errors after cancel/focus/unmount and cancels only once", async () => {
    const { tester, load, stop, apply, fail } = setup();
    const read = deferred<IngressMatch>();
    load.mockReturnValue(read.promise);
    const pending = tester.test("https://app.example.com");
    tester.cancel();
    tester.cancel();
    read.reject("cancelled");
    await pending;
    expect(stop).toHaveBeenCalledTimes(1);
    expect(apply).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
  });
  it.each([
    { ...result(), source_path: "/tmp/other.yml" },
    { ...result(), rule_index: -1 },
    { ...result(), rule_index: 2 },
    { ...result(), rule_index: 0.5 },
    { ...result(), rules: [] },
  ])("rejects inconsistent source/index replies: %j", async (reply) => {
    const { tester, load, apply, fail } = setup();
    load.mockResolvedValue(reply);
    await tester.test("https://app.example.com");
    expect(apply).not.toHaveBeenCalled();
    expect(fail).toHaveBeenCalledExactlyOnceWith("The config or rule result changed. Test the URL again.");
  });
  it("surfaces current safe backend errors and never invokes unsupported tunnel kinds", async () => {
    const { tester, load, fail, select } = setup();
    load.mockRejectedValue("This config has no local ingress rules.");
    await tester.test("https://app.example.com");
    expect(fail).toHaveBeenCalledExactlyOnceWith("This config has no local ingress rules.");
    select(null);
    await tester.test("https://app.example.com");
    expect(load).toHaveBeenCalledTimes(1);
  });
});

describe("source versus displayed rules", () => {
  it("ignores inspection metadata but retains order, catch-all, hostname, path and source service", () => {
    const rules = result().rules;
    expect(sameIngressRules(rules, rules.map((r) => ({ ...r, inspected: true, note: "inspection enabled" })))).toBe(true);
    expect(sameIngressRules(rules, [...rules].reverse())).toBe(false);
    expect(sameIngressRules(rules, rules.slice(0, 1))).toBe(false);
    expect(sameIngressRules(rules, [rule("other.example.com"), rules[1]])).toBe(false);
    expect(sameIngressRules(rules, [rule("app.example.com", "http://localhost:3000", "/api"), rules[1]])).toBe(false);
    expect(sameIngressRules(rules, [rule("app.example.com", "http://127.0.0.1:53333"), rules[1]])).toBe(false);
  });
});
