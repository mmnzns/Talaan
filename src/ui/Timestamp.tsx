import type { Ms } from "../data/types";
import { formatTime } from "../lib/format";
import { cn } from "./cn";

interface Props {
  at: Ms;
  onJump?: (at: Ms) => void;
  className?: string;
}

/** A clickable mono time pill. Jumps to that moment in the transcript. */
export function Timestamp({ at, onJump, className }: Props) {
  const cls = cn(
    "flex-none rounded border border-line bg-paper px-1.5 py-0.5 font-mono text-[11px] leading-[14px] text-subtle",
    onJump && "cursor-pointer hover:border-edge hover:text-ink",
    className,
  );
  return onJump ? (
    <button type="button" className={cls} onClick={() => onJump(at)} aria-label={`Jump to ${formatTime(at)}`}>
      {formatTime(at)}
    </button>
  ) : (
    <span className={cls}>{formatTime(at)}</span>
  );
}
