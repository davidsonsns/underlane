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
    if (!el?.isConnected) return; // e.g. the trigger's row was deleted: let Bits decide
    e.preventDefault();
    el.focus({ preventScroll: true });
  };
}
