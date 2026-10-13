import type { IngressMatch, RouteMapping } from "./types";

export interface IngressTarget {
  id: string;
  source_path: string;
  run: number;
  config_revision: number;
}

/** Match only ordered source rules, never an inspecting proxy's derived configuration. */
export function sameIngressRules(source: RouteMapping[], displayed: RouteMapping[]): boolean {
  return source.length === displayed.length && source.every((rule, i) =>
    rule.hostname === displayed[i].hostname && rule.path === displayed[i].path && rule.service === displayed[i].service);
}

/** Cancel the child and discard delayed replies when the selected source or run changes. */
export function ingressTester(
  target: () => IngressTarget | null,
  load: (id: string, url: string, requestId: string) => Promise<IngressMatch>,
  stop: (requestId: string) => Promise<void>,
  apply: (result: IngressMatch) => void,
  fail: (error: unknown) => void,
) {
  let pending: string | null = null;
  function cancel() {
    const requestId = pending;
    pending = null;
    if (requestId) void stop(requestId).catch(() => {});
  }
  async function test(url: string) {
    cancel();
    const snapshot = target();
    if (!snapshot) return;
    const key = JSON.stringify(snapshot);
    const requestId = crypto.randomUUID();
    pending = requestId;
    const current = () => pending === requestId && JSON.stringify(target()) === key;
    try {
      const result = await load(snapshot.id, url, requestId);
      if (current()) {
        if (result.source_path !== snapshot.source_path || !Number.isInteger(result.rule_index) ||
          result.rule_index < 0 || result.rule_index >= result.rules.length) {
          fail("The config or rule result changed. Test the URL again.");
        } else apply(result);
      }
    } catch (error) {
      if (current()) fail(error);
    } finally {
      if (pending === requestId) pending = null;
    }
  }
  return { test, cancel };
}
