import { useState, type ReactNode } from "react";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Field, Label, cn } from "../../ui";

/** Section heading with a short explanation, used above a group of settings. */
export function SectionIntro({ title, desc }: { title: string; desc?: string }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h2 className="m-0 font-display text-[22px] font-bold leading-7 tracking-[-0.02em]">{title}</h2>
      {desc && <p className="m-0 max-w-[560px] text-[13px] leading-[19px] text-subtle">{desc}</p>}
    </div>
  );
}

/** A card of settings rows separated by hairlines. */
export function Group({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col divide-y divide-line rounded-card border border-line bg-surface px-[22px] shadow-card", className)}>{children}</div>
  );
}

export function Row({ title, desc, children, stack }: { title: string; desc?: ReactNode; children?: ReactNode; stack?: boolean }) {
  return (
    <div className={cn("flex gap-x-5 gap-y-2.5 py-4", stack ? "flex-col" : "flex-wrap items-center justify-between")}>
      <div className={cn("min-w-0", !stack && "flex-1 basis-56")}>
        <div className="font-semibold">{title}</div>
        {desc && <div className="mt-0.5 text-xs leading-[17px] text-subtle">{desc}</div>}
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
    <div role="radiogroup" aria-label={label} className="inline-flex flex-none gap-0.5 rounded-[10px] border border-line bg-rail p-[3px]">
      {options.map((o) => {
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
              "h-7 cursor-pointer rounded-[7px] border-0 px-3 font-[inherit] text-xs font-semibold",
              on ? "bg-strong text-strong-fg" : "bg-transparent text-subtle hover:text-ink",
              o.disabled && "cursor-not-allowed opacity-50 hover:text-subtle",
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
  const t = useT();
  return (
    <span className="flex items-center gap-1.5">
      <Label>{label}</Label>
      <span className="flex gap-[3px]" role="img" aria-label={t("settings.dots_aria", { label, n })}>
        {[1, 2, 3].map((i) => (
          <span key={i} className={cn("size-2 rounded-full", i <= n ? "bg-gold" : "bg-line")} />
        ))}
      </span>
    </span>
  );
}
