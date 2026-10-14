import type { InputHTMLAttributes } from "react";
import { cn } from "./cn";

type Props = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "role" | "onChange"> & {
  onChange?: (checked: boolean) => void;
};

export function Switch({ className, onChange, ...rest }: Props) {
  return (
    <input
      type="checkbox"
      role="switch"
      onChange={(e) => onChange?.(e.target.checked)}
      className={cn(
        "m-0 inline-grid h-[22px] w-10 flex-none cursor-pointer appearance-none place-content-center items-center justify-items-start rounded-full border-[1.5px] border-input bg-transparent px-[3px]",
        "before:size-3.5 before:rounded-full before:bg-subtle before:transition-transform before:content-['']",
        "checked:border-edge checked:bg-lime checked:before:translate-x-[17px] checked:before:bg-on-lime",
        "outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink disabled:cursor-default disabled:opacity-50",
        className,
      )}
      {...rest}
    />
  );
}
