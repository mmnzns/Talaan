import { useState, type ReactNode } from "react";
import { Button, Field, Label, cn } from "../../ui";

/** Section heading with a short explanation, used above a group of settings. */
export function SectionIntro({ title, desc }: { title: string; desc?: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <h3 className="m-0 text-[15px] font-extrabold tracking-tight">{title}</h3>
      {desc && <p className="m-0 text-[13px] leading-[18px] text-subtle">{desc}</p>}
    </div>
  );
}

/** A card of settings rows separated by hairlines. */
export function Group({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col divide-y divide-line rounded-card border-[1.5px] border-edge bg-surface", className)}>{children}</div>
  );
}

export function Row({ title, desc, children, stack }: { title: string; desc?: ReactNode; children?: ReactNode; stack?: boolean }) {
  return (
    <div className={cn("flex gap-x-4 gap-y-2 px-4 py-3", stack ? "flex-col" : "flex-wrap items-center justify-between")}>
      <div className="min-w-0 flex-1 basis-56">
        <div className="text-[13px] font-semibold">{title}</div>
        {desc && <div className="text-xs leading-[17px] text-subtle">{desc}</div>}
      </div>
      {children}
    </div>
  );
}

/** Ask / Always / Never style segmented control. */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string; disabled?: boolean; title?: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex overflow-hidden rounded-control border-[1.5px] border-edge">
      {options.map((o, i) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            disabled={o.disabled}
            title={o.title}
            onClick={() => onChange(o.value)}
            className={cn(
              "h-7 cursor-pointer border-0 px-2.5 font-[inherit] text-xs font-semibold",
              i > 0 && "border-l-[1.5px] border-l-edge",
              on ? "bg-strong text-strong-fg" : "bg-surface text-ink hover:bg-lime-soft",
              o.disabled && "cursor-not-allowed bg-sunken text-subtle opacity-60 hover:bg-sunken",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/** A "+ Add something" button that turns into a one-line field. */
export function InlineAdd({ label, placeholder, onAdd }: { label: string; placeholder: string; onAdd: (value: string) => void }) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState("");
  const commit = () => {
    const v = value.trim();
    setOpen(false);
    setValue("");
    if (v) onAdd(v);
  };
  if (!open)
    return (
      <Button variant="ghost" className="self-start text-subtle" onClick={() => setOpen(true)}>
        {label}
      </Button>
    );
  return (
    <Field
      label={placeholder}
      autoFocus
      placeholder={placeholder}
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") {
          setValue("");
          setOpen(false);
        }
      }}
      wrapperClassName="w-60"
    />
  );
}

export function Dots({ n, label }: { n: number; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <Label>{label}</Label>
      <span className="flex gap-[3px]" role="img" aria-label={`${label} ${n} of 3`}>
        {[1, 2, 3].map((i) => (
          <span key={i} className={cn("size-2 rounded-full border border-edge", i <= n ? "bg-ink" : "bg-transparent")} />
        ))}
      </span>
    </span>
  );
}
