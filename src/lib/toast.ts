// The only import site for svelte-sonner's `toast` (02 §1.4). Each window mounts one
// <Toaster /> (ui/sonner); screens call notify.* for events. Persistent state (validation,
// a stopped tunnel) stays inline, not in a toast.
import { toast } from "svelte-sonner";
import { errorText } from "./api";

export const notify = {
  /** "Copied <what>". Repeated copies replace each other instead of stacking. */
  copied(what: string) {
    toast.success(`Copied ${what}`, { id: "copied", duration: 1400 });
  },
  /** An error event. The message is the description and can be selected and copied. */
  error(e: unknown, title = "Something went wrong") {
    toast.error(title, { description: errorText(e), classes: { description: "select-text cursor-text" } });
  },
  /** Settings were written. One toast however fast the user types. */
  saved() {
    toast.success("Saved", { id: "settings-saved", duration: 1200 });
  },
  /** Drop every toast, e.g. when the popover hides so a stale one doesn't reappear. */
  dismiss() {
    toast.dismiss();
  },
};
