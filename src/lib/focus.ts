// Return focus explicitly when a dialog/popover closes (02 R7). WebKit never focuses a
// <button> on click, so Bits' default (restore whatever was focused when it opened) lands
// on <body> after a mouse open. Store the trigger yourself and pass this as
// onCloseAutoFocus:
//
//   let trigger = $state<HTMLElement | null>(null);
//   <Dialog.Content onCloseAutoFocus={returnFocusTo(() => trigger)}>
export function returnFocusTo(getEl: () => HTMLElement | null | undefined) {
  return (e: Event) => {
    const el = getEl();
    if (!el?.isConnected) {
      return; // e.g. the trigger's row was deleted: let Bits decide
    }
    e.preventDefault();
    el.focus({ preventScroll: true });
  };
}

// What a click focuses in WebKit (it never focuses a plain <button> on click).
const CLICK_FOCUSABLE =
  "input, textarea, select, [contenteditable=''], [contenteditable='true'], [tabindex]:not([tabindex='-1'])";

// For a layer that an outside click closes (the Combobox popover). Bits runs onCloseAutoFocus
// for that dismissal too, and its focus trap has already pulled focus back inside, so a plain
// returnFocusTo() would yank focus from the field the user just clicked into back to the
// trigger. Remember what the outside click hit and focus that instead. Spread on the Content:
//
//   <Popover.Content {...returnFocusOnClose(() => trigger)}>
export function returnFocusOnClose(
  getEl: () => HTMLElement | null | undefined
) {
  let clicked: HTMLElement | null = null;
  const fallback = returnFocusTo(getEl);
  return {
    onCloseAutoFocus: (e: Event) => {
      const target = clicked;
      clicked = null;
      if (target?.isConnected) {
        e.preventDefault();
        target.focus({ preventScroll: true });
        return;
      }
      fallback(e);
    },
    onInteractOutside: (e: Event) => {
      const t = e.target as Element | null;
      clicked = t?.closest?.<HTMLElement>(CLICK_FOCUSABLE) ?? null;
    },
  };
}
