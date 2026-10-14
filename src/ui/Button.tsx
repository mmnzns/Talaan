import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

type Variant = "default" | "primary" | "strong" | "danger" | "ghost";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  /** Hard offset shadow, used for the main action in a group. */
  raised?: boolean;
  size?: "md" | "lg" | "icon";
}

const variants: Record<Variant, string> = {
  default: "bg-surface text-ink hover:bg-lime-soft",
  primary: "bg-lime text-on-lime hover:bg-lime shadow-hard",
  strong: "bg-strong text-strong-fg hover:bg-strong",
  danger: "bg-surface text-danger border-danger hover:bg-warn-bg",
  ghost: "bg-transparent border-transparent text-ink hover:bg-hover",
};

const sizes = {
  md: "h-8 px-3",
  lg: "h-[34px] px-3.5",
  icon: "h-8 w-8 px-0",
};

export function Button({ variant = "default", raised, size = "md", className, type = "button", ...rest }: Props) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex cursor-pointer items-center justify-center gap-2 rounded-control border-[1.5px] border-edge text-[13px] font-semibold",
        "outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink disabled:cursor-default disabled:opacity-50",
        variants[variant],
        sizes[size],
        raised && "shadow-hard",
        className,
      )}
      {...rest}
    />
  );
}
