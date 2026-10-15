import { AlertTriangle, Pin } from "lucide-react";
import type { MeetingListItem } from "../../data/types";
import { formatClock, formatDuration } from "../../lib/format";
import { t as tr } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { navigate, routes } from "../../lib/router";
import { cn } from "../../ui";

export function metaLine(m: MeetingListItem): string {
  const parts = [m.appLabel];
  if (m.app !== "import") parts.push(formatClock(m.startedAt));
  parts.push(formatDuration(m.durationMs));
  if (m.peopleCount > 2) parts.push(tr("shell.people", { count: m.peopleCount }));
  return parts.join(" · ").toLowerCase();
}

const PROCESSING = { transcribing: "shell.statusTranscribing", summarizing: "shell.statusSummarizing" } as const;

export function MeetingItem({ meeting, selected }: { meeting: MeetingListItem; selected: boolean }) {
  const t = useT();
  const processing = meeting.status === "transcribing" || meeting.status === "summarizing";
  const go = () => navigate(routes.meeting(meeting.id));

  const base = "flex w-full cursor-pointer flex-col gap-1 rounded-xl border-0 px-3.5 py-[11px] text-left font-[inherit] text-ink";
  const tone = selected ? "bg-surface shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "bg-transparent hover:bg-hover";

  if (processing) {
    return (
      <button type="button" onClick={go} className={cn(base, "gap-1.5", tone)} aria-current={selected ? "page" : undefined}>
        <span className="font-semibold">{meeting.title}</span>
        <span className="flex items-center gap-2">
          <span className={cn("h-[3px] flex-1 overflow-hidden rounded-full", "bg-line")}>
            <span className={cn("block h-full", "bg-gold")} style={{ width: `${Math.round((meeting.progress ?? 0) * 100)}%` }} />
          </span>
          <span className={cn("font-mono text-[11px]", "text-faint")}>
            {t(PROCESSING[meeting.status as keyof typeof PROCESSING])}
          </span>
        </span>
      </button>
    );
  }

  return (
    <button type="button" onClick={go} className={cn(base, tone)} aria-current={selected ? "page" : undefined}>
      <span className={cn("flex items-center gap-1.5", selected ? "font-bold" : "font-semibold")}>
        <span className="min-w-0 flex-1 truncate">{meeting.title}</span>
        {meeting.pinned && <Pin size={12} aria-label={t("shell.audioPinned")} className="flex-none" />}
      </span>
      {meeting.status === "recording" ? (
        <span className={cn("flex items-center gap-1.5 font-mono text-[11px]", "text-red")}>
          <span className="size-1.5 animate-pulse rounded-full bg-current" /> {t("shell.statusRecording")}
        </span>
      ) : meeting.status === "failed" ? (
        <span className={cn("flex items-center gap-1 font-mono text-[11px]", "text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> {t("shell.statusFailed")}
        </span>
      ) : meeting.status === "needs_recovery" ? (
        <span className={cn("flex items-center gap-1 font-mono text-[11px]", "text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> {t("shell.statusNeedsRecovery")}
        </span>
      ) : (
        <span className={cn("font-mono text-[11px]", "text-faint")}>{metaLine(meeting)}</span>
      )}
    </button>
  );
}
