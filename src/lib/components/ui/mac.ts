// Shared macOS class recipes for the shadcn components. The select, dropdown-menu and
// context-menu files all import from here, so the menu look is tuned in one place.

/** Keyboard focus ring for buttons, pop-ups, switches and segments: only after Tab navigation. */
export const kbRing = "outline-none kb-focus:ring-[3px] kb-focus:ring-ring/50 kb-focus:aria-expanded:ring-0";

/** Floating menu surface (dropdown, context, select): translucent material, no zoom. */
export const menuContent =
  "z-50 min-w-[10rem] overflow-x-hidden overflow-y-auto rounded-lg bg-popover p-[5px] shadow-lg text-sm text-popover-foreground border border-input outline-none backdrop-blur-xl backdrop-saturate-150 " +
  // Open instantly like NSMenu; fade out quickly on close.
  "data-closed:animate-out data-closed:fade-out-0 data-closed:duration-150";

/** Menu row: 28px, check column on the left and a neutral hover highlight. */
export const menuItem =
  "group/menu-item relative flex h-7 cursor-default items-center gap-2 rounded-sm ps-[22px] pe-3 text-sm whitespace-nowrap outline-none select-none " +
  "data-highlighted:bg-accent data-highlighted:text-accent-foreground " +
  "data-disabled:pointer-events-none data-disabled:text-faint " +
  "data-[variant=destructive]:text-destructive data-[variant=destructive]:data-highlighted:text-accent-foreground " +
  "data-inset:ps-[22px] [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5";

/** Left check column. */
export const menuIndicator = "pointer-events-none absolute start-[6px] flex w-3 items-center justify-center [&_svg]:size-3 [&_svg]:stroke-[2.5]";

export const menuSeparator = "pointer-events-none mx-[5px] my-[5px] h-0 border-t border-border";

export const menuLabel = "ps-[22px] pe-3 pt-1 pb-0.5 text-xs font-semibold text-muted-foreground";

export const menuShortcut =
  "ms-auto ps-6 text-sm tracking-normal text-muted-foreground group-data-highlighted/menu-item:text-accent-foreground";
