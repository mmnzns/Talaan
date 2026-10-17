import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "./cn";

interface Props {
  trigger: (state: { open: boolean; toggle: () => void }) => ReactNode;
  children: (close: () => void) => ReactNode;
  align?: "left" | "right";
  className?: string;
}

/** Small anchored panel. Closes on Escape and on a click outside. */
export function Popover({ trigger, children, align = "right", className }: Props) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative inline-flex">
      {trigger({ open, toggle: () => setOpen((o) => !o) })}
      {open && (
        <div
          className={cn(
            "absolute top-full z-40 mt-1.5 min-w-[200px] rounded-card border border-edge bg-raised p-1.5 shadow-card",
            align === "right" ? "right-0" : "left-0",
            className,
          )}
        >
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

/** A hairline between groups of menu items. */
export function MenuSeparator() {
  return <div role="separator" className="my-1 border-t border-line" />;
}

export function MenuItem({ className, ...rest }: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className={cn(
        "flex w-full cursor-pointer items-center gap-2 rounded-control border-0 bg-transparent px-2.5 py-1.5 text-left font-[inherit] text-[13px] text-ink hover:bg-hover disabled:cursor-default disabled:opacity-50",
        className,
      )}
      {...rest}
    />
  );
}
