import { describe, expect, it } from "vitest";
import { cn, tv } from "./utils";

// R-19 / R22: the custom text sizes must survive next to a colour, and still override text-sm.
describe("cn knows the custom text sizes", () => {
  it("keeps text-callout next to a colour", () => {
    expect(cn("text-callout text-muted-foreground")).toBe("text-callout text-muted-foreground");
  });
  it("keeps text-code next to a colour", () => {
    expect(cn("text-code", "text-foreground")).toBe("text-code text-foreground");
  });
  it("keeps text-2xs next to a status colour", () => {
    expect(cn("text-2xs font-medium", "text-ok-text")).toBe("text-2xs font-medium text-ok-text");
  });
  it("lets a custom size replace text-sm (and a colour replace a colour)", () => {
    expect(cn("text-sm text-foreground", "text-callout text-muted-foreground")).toBe("text-callout text-muted-foreground");
  });
});

describe("tv does not merge on its own", () => {
  it("leaves conflicting classes for cn to resolve", () => {
    const v = tv({ base: "text-sm text-foreground", variants: { s: { small: "text-callout" } } });
    expect(v({ s: "small" })).toBe("text-sm text-foreground text-callout");
    expect(cn(v({ s: "small" }), "text-muted-foreground")).toBe("text-callout text-muted-foreground");
  });
});
