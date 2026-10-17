import { ChevronDown } from "lucide-react";
import type { SelectHTMLAttributes } from "react";
import { cn } from "./cn";

interface Props extends SelectHTMLAttributes<HTMLSelectElement> {
  label: string;
  wrapperClassName?: string;
}

export function Select({ label, className, wrapperClassName, children, ...rest }: Props) {
  return (
    <label className={cn("relative inline-flex max-w-full", wrapperClassName)}>
      <span className="sr-only">{label}</span>
      <select
        className={cn(
          "h-8 w-full min-w-0 cursor-pointer appearance-none truncate rounded-control border border-input bg-paper pl-2.5 pr-8 font-[inherit] text-[13px] text-ink outline-none focus:border-edge",
          className,
        )}
        {...rest}
      >
        {children}
      </select>
      <ChevronDown aria-hidden="true" size={14} className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-subtle" />
    </label>
  );
}
