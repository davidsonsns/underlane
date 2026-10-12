import { createCn } from "cn/config";
import { tv as tvLite } from "tailwind-variants/lite";

// Class merging that knows Underlane's custom text sizes. Unconfigured, `cn` treats text-2xs,
// text-callout and text-code as colours and drops them next to a real colour
// (cn("text-callout text-muted-foreground") -> "text-muted-foreground"). See utils.test.ts.
export const cn = createCn({ extend: { classGroups: { "font-size": [{ text: ["2xs", "callout", "code"] }] } } });

// Variants never merge on their own (the `twMerge: false` of tailwind-variants 3): callers wrap
// them in cn(variants(…), className), which merges once with the configured engine above.
// The lite build is tailwind-variants with merging off, so its merge engine isn't bundled.
export const tv = tvLite;
export type { VariantProps } from "tailwind-variants/lite";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
