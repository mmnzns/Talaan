import type { Ms } from "../data/types";
import { formatTime } from "../lib/format";
import { useT } from "../lib/i18n/I18nProvider";
import { cn } from "./cn";

interface Props {
  at: Ms;
  onJump?: (at: Ms) => void;
  /** The moment being shown: drawn in sun gold. */
  active?: boolean;
  className?: string;
}

/** A clickable mono time pill. Jumps to that moment in the transcript. */
export function Timestamp({ at, onJump, active, className }: Props) {
  const t = useT();
  const cls = cn(
    "flex-none rounded-md border border-edge bg-transparent px-1.5 py-0.5 font-mono text-[11px] leading-[14px] text-faint",
    onJump && "cursor-pointer hover:border-gold hover:bg-gold hover:text-on-gold",
    active && "border-gold bg-gold text-on-gold",
    className,
  );
  return onJump ? (
    <button type="button" className={cls} onClick={() => onJump(at)} aria-label={t("kit.jumpTo", { time: formatTime(at) })}>
      {formatTime(at)}
    </button>
  ) : (
    <span className={cls}>{formatTime(at)}</span>
  );
}
