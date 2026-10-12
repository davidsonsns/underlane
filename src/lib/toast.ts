// The only import site for svelte-sonner's `toast` (02 §1.4). Each window mounts one
// <Toaster /> (ui/sonner); screens call notify.* for events. Persistent state (validation,
// a stopped tunnel) stays inline, not in a toast.
//
// svelte-sonner loads on the first toast (02 O10): the popover's first open doesn't pay for
// it. The dashboard imports the Toaster statically, so there the import resolves at once.
import type { toast as Toast } from "svelte-sonner";
import { errorText } from "./api";

let sonner: Promise<typeof Toast> | undefined;
let loaded: (() => void) | undefined;

function show(fn: (toast: typeof Toast) => void) {
  sonner ??= import("./lazy").then((m) => (loaded?.(), m.toast));
  sonner.then(fn);
}

export const notify = {
  /** "Copied <what>". Repeated copies replace each other instead of stacking. */
  copied(what: string) {
    show((t) => t.success(`Copied ${what}`, { id: "copied", duration: 1400 }));
  },
  /** An error event. The message is the description and can be selected and copied. */
  error(e: unknown, title = "Something went wrong") {
    show((t) => t.error(title, { description: errorText(e), classes: { description: "select-text cursor-text" } }));
  },
  /** Settings were written. One toast however fast the user types. */
  saved() {
    show((t) => t.success("Saved", { id: "settings-saved", duration: 1200 }));
  },
  /** Drop every toast, e.g. when the popover hides so a stale one doesn't reappear. */
  dismiss() {
    sonner?.then((t) => t.dismiss());
  },
  /** Runs once svelte-sonner has loaded for the first toast. The popover mounts its Toaster
   *  then (a Toaster mounted after a toast() call still shows it: the toast list is global). */
  onLoad(fn: () => void) {
    loaded = fn;
  },
};
