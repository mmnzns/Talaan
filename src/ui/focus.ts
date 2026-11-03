/** Focus helpers for modal dialogs (Sheet). */

const FOCUSABLE = 'a[href],button,input,select,textarea,summary,[tabindex]';

function isTabbable(el: HTMLElement): boolean {
  if (el.tabIndex < 0) return false;
  if ((el as HTMLButtonElement).disabled) return false;
  if (el instanceof HTMLInputElement && el.type === "hidden") return false;
  if (el.closest("[hidden],[inert]")) return false;
  // Skipped where the engine has no checkVisibility (jsdom); real webviews do.
  if (typeof el.checkVisibility === "function" && !el.checkVisibility()) return false;
  return true;
}

/** Tab stops inside `root`, in DOM order. */
export function focusableIn(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(isTabbable);
}

/**
 * Where Tab should go when the browser's own move would leave the dialog (or focus is not on a tab stop inside
 * it). Returns the index to focus, or `null` to let the browser move focus normally.
 * `current` is the index of the focused tab stop, or -1 when focus is on the dialog itself or outside it.
 */
export function trapTarget(count: number, current: number, shift: boolean): number | null {
  if (count === 0) return null;
  if (current < 0) return shift ? count - 1 : 0;
  if (shift && current === 0) return count - 1;
  if (!shift && current === count - 1) return 0;
  return null;
}

/** The dialog's preferred first control: `data-autofocus`, else the first tab stop, else nothing. */
export function initialFocusIn(root: HTMLElement): HTMLElement | null {
  return root.querySelector<HTMLElement>("[data-autofocus]") ?? focusableIn(root)[0] ?? null;
}

// WebKit (the Mac webview) does not focus a button when it is clicked, so remember what was pressed.
let lastPointer: { el: HTMLElement; at: number } | null = null;
let tracking = false;
const POINTER_MEMORY_MS = 2000;

export function trackPointer() {
  if (tracking || typeof document === "undefined") return;
  tracking = true;
  document.addEventListener(
    "mousedown",
    (e) => {
      const el = (e.target as Element | null)?.closest?.<HTMLElement>("button,a[href],[role=button],[role=menuitem],summary,[tabindex]");
      lastPointer = el ? { el, at: Date.now() } : null;
    },
    true,
  );
}

/** The element to give focus back to when a dialog closes. */
export function openerElement(): HTMLElement | null {
  const active = document.activeElement;
  if (active instanceof HTMLElement && active !== document.body) return active;
  if (lastPointer && Date.now() - lastPointer.at < POINTER_MEMORY_MS && lastPointer.el.isConnected) return lastPointer.el;
  return null;
}
