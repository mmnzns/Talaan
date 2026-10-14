import type { ReactNode } from "react";
import { Label } from "./Label";

interface Props {
  title: string;
  hint?: ReactNode;
  action?: ReactNode;
}

export function SectionHeader({ title, hint, action }: Props) {
  return (
    <div className="flex items-baseline gap-3">
      <h2 className="m-0 text-[15px] font-extrabold tracking-tight">{title}</h2>
      {hint && <Label>{hint}</Label>}
      <span className="flex-1" />
      {action}
    </div>
  );
}
