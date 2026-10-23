import { useEffect, useRef, type ReactNode } from "react";
import { cn } from "./cn";
import { focusableIn, initialFocusIn, openerElement, trackPointer, trapTarget } from "./focus";

// Remember clicks from the start: on the Mac a clicked button doesn't take focus, so the click that opens the first
// sheet is the only way to know where focus should go back to.
trackPointer();

interface Props {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}

/**
 * Modal sheet with a soft shadow. Closes on Escape and on a click outside. Focus starts on the control marked
 * `data-autofocus` (else the first control), Tab stays inside, and focus goes back to the opener on close.
 */
export function Sheet({ open, title, onClose, children, className }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  // Focus in on open, back to the opener on close. Keyed on `open` only: a parent re-rendering with a new
  // `onClose` must not move focus.
  useEffect(() => {
    if (!open) return;
    const opener = openerElement();
    const dialog = ref.current;
    if (dialog && !dialog.contains(document.activeElement)) (initialFocusIn(dialog) ?? dialog).focus();
    return () => {
      const active = document.activeElement;
      if (opener?.isConnected && (!active || active === document.body)) opener.focus({ preventScroll: true });
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      // Another sheet already took this key (two stacked sheets each listen; the top one can close first).
      if (e.defaultPrevented || (e.key !== "Escape" && e.key !== "Tab")) return;
      const dialog = ref.current;
      if (!dialog) return;
      // Only the top-most sheet answers Escape and traps Tab (a sheet can open over another, e.g. a failed import).
      const modals = document.querySelectorAll('[aria-modal="true"]');
      if (modals[modals.length - 1] !== dialog) return;
      if (e.key === "Escape") {
        e.preventDefault();
        onCloseRef.current();
        return;
      }
      const stops = focusableIn(dialog);
      if (stops.length === 0) {
        e.preventDefault();
        dialog.focus();
        return;
      }
      const target = trapTarget(stops.length, stops.indexOf(document.activeElement as HTMLElement), e.shiftKey);
      if (target !== null) {
        e.preventDefault();
        stops[target].focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/30 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        className={cn(
          "max-h-full w-full max-w-[460px] overflow-auto rounded-card border border-edge bg-raised shadow-pop outline-none",
          className,
        )}
      >
        {children}
      </div>
    </div>
  );
}
