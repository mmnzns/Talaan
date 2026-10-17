import { ChevronDown } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useT } from "../../lib/i18n/I18nProvider";
import { cn } from "../../ui";

/**
 * Keeps a long list calm: the first `limit` rows, and "Show all N" for the rest. Short lists show everything.
 * `children` gets how many rows to render.
 */
export function Clamp({ count, limit, children }: { count: number; limit: number; children: (shown: number) => ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  // One hidden row isn't worth a button.
  const long = count > limit + 1;
  return (
    <>
      {children(long && !open ? limit : count)}
      {long && (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
          className="-mx-1 flex cursor-pointer items-center gap-1.5 self-start rounded-md border-0 bg-transparent px-1 py-0.5 font-[inherit] text-xs font-semibold text-subtle outline-offset-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-ink"
        >
          <ChevronDown size={13} aria-hidden="true" className={cn("transition-transform", open && "rotate-180")} />
          {open ? t("meeting.showLess") : t("meeting.showAll", { count })}
        </button>
      )}
    </>
  );
}
