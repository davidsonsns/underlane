import { describe, expect, it } from "vitest";
import { cn, tv } from "./utils";

// R-19 / R22: the custom text sizes must survive next to a colour, and still override text-sm.
describe("cn knows the custom text sizes", () => {
  it("keeps text-callout next to a colour", () => {
    expect(cn("text-callout text-muted-foreground")).toBe(
      "text-callout text-muted-foreground"
    );
  });
  it("keeps text-code next to a colour", () => {
    expect(cn("text-code", "text-foreground")).toBe(
      "text-code text-foreground"
    );
  });
  it("keeps text-2xs next to a status colour", () => {
    expect(cn("font-medium text-2xs", "text-ok-text")).toBe(
      "font-medium text-2xs text-ok-text"
    );
  });
  it("lets a custom size replace text-sm (and a colour replace a colour)", () => {
    expect(
      cn("text-foreground text-sm", "text-callout text-muted-foreground")
    ).toBe("text-callout text-muted-foreground");
  });
});

describe("tv does not merge on its own", () => {
  it("leaves conflicting classes for cn to resolve", () => {
    const v = tv({
      base: "text-foreground text-sm",
      variants: { s: { small: "text-callout" } },
    });
    expect(v({ s: "small" })).toBe("text-foreground text-sm text-callout");
    expect(cn(v({ s: "small" }), "text-muted-foreground")).toBe(
      "text-callout text-muted-foreground"
    );
  });
});
