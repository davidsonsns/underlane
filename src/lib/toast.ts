// The only import site for svelte-sonner's `toast` (02 §1.4). Each window mounts one
// <Toaster /> (ui/sonner); screens call notify.* for events. Persistent state (validation
// failures, a stopped tunnel) stays inline.
//
// svelte-sonner loads on the first toast (02 O10): the popover's first open doesn't pay for
// it. The dashboard imports the Toaster statically, so there the import resolves at once.
import type { toast as Toast } from "svelte-sonner";
import { errorText } from "./api";

let sonner: Promise<typeof Toast> | undefined;
let isLoaded = false;
let loaded: (() => void) | undefined;

function show(fn: (toast: typeof Toast) => void) {
  // A failed load is forgotten so the next toast retries it, instead of every later toast
  // silently failing on the same rejected promise.
  sonner ??= import("./lazy").then(
    (m) => {
      isLoaded = true;
      loaded?.();
      return m.toast;
    },
    (e) => {
      sonner = undefined;
      throw e;
    },
  );
  sonner.then(fn, (e) => console.error("Couldn't load toasts", e));
}

export const notify = {
  /** A successful action. */
  success(message: string) {
    show((t) => t.success(message));
  },
  /** "Copied <what>". Repeated copies replace each other instead of stacking. */
  copied(what: string) {
    show((t) => t.success(`Copied ${what}`, { id: "copied", duration: 1400 }));
  },
  /** An error event. The message is the description and can be selected and copied. */
  error(e: unknown, title = "Something went wrong") {
    show((t) => t.error(title, { description: errorText(e), classes: { description: "select-text cursor-text" } }));
  },
  /** Drop every toast, e.g. when the popover hides so a stale one doesn't reappear. */
  dismiss() {
    sonner?.then((t) => t.dismiss(), () => {});
  },
  /** Runs once svelte-sonner has loaded for the first toast. The popover mounts its Toaster
   *  then (a Toaster mounted after a toast() call still shows it: the toast list is global). */
  onLoad(fn: () => void) {
    loaded = fn;
    if (isLoaded) fn();
  },
};
