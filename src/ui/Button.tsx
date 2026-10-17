import type { ComponentPropsWithRef } from "react";
import { cn } from "./cn";

type Variant = "default" | "primary" | "strong" | "danger" | "ghost";

/** A plain button's props, `ref` included (React 19 passes it like any prop). */
interface Props extends ComponentPropsWithRef<"button"> {
  variant?: Variant;
  /** Soft shadow, used for the main action in a group. */
  raised?: boolean;
  size?: "md" | "lg" | "icon";
}

const variants: Record<Variant, string> = {
  default: "bg-surface text-ink hover:bg-hover",
  /** Primary buttons are ink, never a color. */
  primary: "border-strong bg-strong text-strong-fg hover:brightness-125",
  strong: "border-strong bg-strong text-strong-fg hover:brightness-125",
  danger: "bg-surface text-danger border-danger hover:bg-warn-bg",
  ghost: "bg-transparent border-transparent text-ink hover:bg-hover",
};

const sizes = {
  md: "h-8 px-[13px]",
  lg: "h-[36px] px-4",
  icon: "h-8 w-8 px-0",
};

export function Button({ variant = "default", raised, size = "md", className, type = "button", ...rest }: Props) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex cursor-pointer items-center justify-center gap-2 rounded-control border border-edge text-[13px] font-semibold",
        "outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink disabled:cursor-default disabled:opacity-50",
        variants[variant],
        sizes[size],
        raised && "shadow-card",
        className,
      )}
      {...rest}
    />
  );
}
