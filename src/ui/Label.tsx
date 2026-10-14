import type { HTMLAttributes } from "react";
import { cn } from "./cn";

/** Mono caption in the muted color: section names, counts, metadata. */
export function Label({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) {
  return <span className={cn("font-mono text-[11px] leading-[14px] text-subtle", className)} {...rest} />;
}
