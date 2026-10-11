// Styled tooltips that appear quickly. WebKit's native `title` tooltip waits about a second
// and is easy to miss, which matters for status dots whose meaning isn't obvious.
// Usage: <span use:tooltip={"text"}>. One shared element serves the whole window.

const DELAY_MS = 250;
const GAP = 6;
const EDGE = 8;

let el: HTMLDivElement | null = null;
let owner: HTMLElement | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

function show(anchor: HTMLElement, text: string) {
  if (!el) {
    el = document.createElement("div");
    el.className = "tooltip";
    el.setAttribute("role", "tooltip");
    document.body.append(el);
  }
  owner = anchor;
  el.textContent = text;
  el.classList.add("visible");
  place(anchor);
}

// Centred above the anchor, flipped below when there's no room, kept inside the window.
function place(anchor: HTMLElement) {
  if (!el) return;
  const a = anchor.getBoundingClientRect();
  const t = el.getBoundingClientRect();
  const left = Math.min(Math.max(a.left + a.width / 2 - t.width / 2, EDGE), window.innerWidth - t.width - EDGE);
  const top = a.top - t.height - GAP >= EDGE ? a.top - t.height - GAP : a.bottom + GAP;
  el.style.left = `${left}px`;
  el.style.top = `${top}px`;
}

function hide() {
  clearTimeout(timer);
  owner = null;
  el?.classList.remove("visible");
}

addEventListener("scroll", hide, true);
addEventListener("blur", hide);

export function tooltip(node: HTMLElement, text: string | undefined) {
  let current = text;
  const enter = () => {
    clearTimeout(timer);
    timer = setTimeout(() => current && show(node, current), DELAY_MS);
  };
  const focus = () => current && node.matches(":focus-visible, :has(:focus-visible)") && show(node, current);

  node.addEventListener("mouseenter", enter);
  node.addEventListener("mouseleave", hide);
  node.addEventListener("mousedown", hide);
  node.addEventListener("focusin", focus);
  node.addEventListener("focusout", hide);

  return {
    update(next: string | undefined) {
      current = next;
      if (owner !== node) return;
      if (next) show(node, next);
      else hide();
    },
    destroy() {
      if (owner === node) hide();
      node.removeEventListener("mouseenter", enter);
      node.removeEventListener("mouseleave", hide);
      node.removeEventListener("mousedown", hide);
      node.removeEventListener("focusin", focus);
      node.removeEventListener("focusout", hide);
    },
  };
}
