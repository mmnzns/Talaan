import { useEffect, useRef, type ReactNode } from "react";
import { cn } from "./cn";

interface Props {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}

/** Modal sheet with a soft shadow. Closes on Escape and on a click outside. */
export function Sheet({ open, title, onClose, children, className }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    ref.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

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
