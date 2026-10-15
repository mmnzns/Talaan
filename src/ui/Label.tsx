import type { HTMLAttributes } from "react";
import { cn } from "./cn";

/** Lowercase mono caption in the faint color: section names, counts, metadata. */
export function Label({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) {
  return <span className={cn("font-mono text-[11px] leading-[14px] text-faint lowercase", className)} {...rest} />;
}

/** A section label with the short gold rule before it ("— decisions"). */
export function SectionLabel({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-2 font-mono text-[11px] leading-[14px] text-faint lowercase before:h-0.5 before:w-3.5 before:rounded-sm before:bg-gold before:content-['']",
        className,
      )}
      {...rest}
    />
  );
}
