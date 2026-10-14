import type { InputHTMLAttributes } from "react";
import { cn } from "./cn";

type Props = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "onChange"> & {
  onChange?: (checked: boolean) => void;
};

export function Checkbox({ className, onChange, ...rest }: Props) {
  return (
    <input
      type="checkbox"
      onChange={(e) => onChange?.(e.target.checked)}
      className={cn(
        "m-0 mt-0.5 inline-grid size-4 flex-none cursor-pointer appearance-none place-content-center rounded-[5px] border-[1.5px] border-input bg-paper",
        "checked:border-edge checked:bg-lime checked:before:size-[9px] checked:before:bg-on-lime checked:before:content-['']",
        "checked:before:[clip-path:polygon(14%_44%,0_65%,50%_100%,100%_16%,80%_0%,43%_62%)]",
        "outline-offset-2 focus-visible:outline-2 focus-visible:outline-ink disabled:cursor-default disabled:opacity-50",
        className,
      )}
      {...rest}
    />
  );
}
