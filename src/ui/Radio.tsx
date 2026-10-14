import { cn } from "./cn";

/** Visual radio dot. The parent element carries the click handler and role="radio". */
export function Radio({ checked, className }: { checked: boolean; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-grid size-[18px] flex-none place-content-center rounded-full border-[1.5px] bg-surface",
        checked ? "border-edge" : "border-input",
        className,
      )}
    >
      <span className={cn("size-[9px] rounded-full", checked ? "bg-ink" : "bg-transparent")} />
    </span>
  );
}
