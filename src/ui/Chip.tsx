import type { HTMLAttributes } from "react";
import { cn } from "./cn";

interface Props extends HTMLAttributes<HTMLElement> {
  tone?: "default" | "strong" | "lime";
  /** Renders a button instead of a span. */
  onClick?: () => void;
}

const tones = {
  default: "border-line bg-surface text-subtle",
  strong: "border-strong bg-strong text-strong-fg",
  lime: "border-edge bg-lime-soft text-ink",
};

export function Chip({ tone = "default", className, onClick, children, ...rest }: Props) {
  const cls = cn(
    "inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-[3px] font-mono text-[11px] leading-[14px]",
    tones[tone],
    onClick && "cursor-pointer hover:border-edge hover:text-ink",
    className,
  );
  return onClick ? (
    <button type="button" className={cls} onClick={onClick} {...(rest as HTMLAttributes<HTMLButtonElement>)}>
      {children}
    </button>
  ) : (
    <span className={cls} {...rest}>
      {children}
    </span>
  );
}
