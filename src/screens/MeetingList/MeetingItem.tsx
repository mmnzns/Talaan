import { AlertTriangle, Pin } from "lucide-react";
import type { MeetingListItem } from "../../data/types";
import { formatClock, formatDuration } from "../../lib/format";
import { navigate, routes } from "../../lib/router";
import { cn } from "../../ui";

export function metaLine(m: MeetingListItem): string {
  const parts = [m.appLabel];
  if (m.app !== "import") parts.push(formatClock(m.startedAt));
  parts.push(formatDuration(m.durationMs));
  if (m.peopleCount > 2) parts.push(`${m.peopleCount} people`);
  return parts.join(" · ");
}

const PROCESSING = { transcribing: "transcribing", summarizing: "summarizing" } as const;

export function MeetingItem({ meeting, selected }: { meeting: MeetingListItem; selected: boolean }) {
  const processing = meeting.status === "transcribing" || meeting.status === "summarizing";
  const go = () => navigate(routes.meeting(meeting.id));

  const base = "flex w-full cursor-pointer flex-col gap-0.5 rounded-control border-0 p-2.5 text-left font-[inherit] text-ink";
  const tone = selected ? "bg-strong text-strong-fg" : "bg-transparent hover:bg-hover";

  if (processing) {
    return (
      <button type="button" onClick={go} className={cn(base, "gap-1.5", tone)} aria-current={selected ? "page" : undefined}>
        <span className="font-semibold">{meeting.title}</span>
        <span className="flex items-center gap-2">
          <span className={cn("h-[5px] flex-1 overflow-hidden rounded-full", selected ? "bg-strong-fg/30" : "bg-line")}>
            <span className={cn("block h-full", selected ? "bg-strong-fg" : "bg-ink")} style={{ width: `${Math.round((meeting.progress ?? 0) * 100)}%` }} />
          </span>
          <span className={cn("font-mono text-[11px]", selected ? "text-strong-fg" : "text-subtle")}>
            {PROCESSING[meeting.status as keyof typeof PROCESSING]}
          </span>
        </span>
      </button>
    );
  }

  return (
    <button type="button" onClick={go} className={cn(base, tone)} aria-current={selected ? "page" : undefined}>
      <span className="flex items-center gap-1.5 font-semibold">
        <span className="min-w-0 flex-1 truncate">{meeting.title}</span>
        {meeting.pinned && <Pin size={12} aria-label="Audio pinned" className="flex-none" />}
      </span>
      {meeting.status === "recording" ? (
        <span className={cn("flex items-center gap-1.5 font-mono text-[11px]", selected ? "text-strong-fg" : "text-ok")}>
          <span className="size-1.5 animate-pulse rounded-full bg-current" /> Recording now
        </span>
      ) : meeting.status === "failed" ? (
        <span className={cn("flex items-center gap-1 font-mono text-[11px]", selected ? "text-strong-fg" : "text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> Processing failed
        </span>
      ) : meeting.status === "needs_recovery" ? (
        <span className={cn("flex items-center gap-1 font-mono text-[11px]", selected ? "text-strong-fg" : "text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> Recording interrupted · Recover
        </span>
      ) : (
        <span className={cn("font-mono text-[11px]", selected ? "text-strong-fg" : "text-subtle")}>{metaLine(meeting)}</span>
      )}
    </button>
  );
}
