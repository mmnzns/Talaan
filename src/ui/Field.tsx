import type { InputHTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

interface Props extends InputHTMLAttributes<HTMLInputElement> {
  /** Accessible name when there is no visible label. */
  label: string;
  icon?: ReactNode;
  wrapperClassName?: string;
}

export function Field({ label, icon, className, wrapperClassName, ...rest }: Props) {
  return (
    <label
      className={cn(
        "flex h-8 items-center gap-2 rounded-control border-[1.5px] border-input bg-paper px-2.5 text-[13px] focus-within:border-edge",
        wrapperClassName,
      )}
    >
      {icon && <span className="text-subtle">{icon}</span>}
      <span className="sr-only">{label}</span>
      <input
        className={cn("min-w-0 flex-1 border-0 bg-transparent font-[inherit] text-[13px] text-ink outline-none placeholder:text-subtle", className)}
        {...rest}
      />
    </label>
  );
}
