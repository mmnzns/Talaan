import { AlertTriangle, Pin } from "lucide-react";
import type { MeetingListItem } from "../../data/types";
import { formatClock, formatDate, formatDuration } from "../../lib/format";
import { t as tr } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { navigate, routes } from "../../lib/router";
import { cn } from "../../ui";
import { etaText, PROCESSING_KEYS, useNow } from "../../lib/eta";

export function metaLine(m: MeetingListItem): string {
  const parts = [m.appLabel];
  if (m.app !== "import") parts.push(formatClock(m.startedAt));
  parts.push(formatDuration(m.durationMs));
  if (m.peopleCount > 2) parts.push(tr("shell.people", { count: m.peopleCount }));
  return parts.join(" · ");
}

/** The line under a meeting's title: the place people scan most, so plain 12 px text rather than the mono label. */
const meta = "text-xs leading-[17px] text-faint";

export function MeetingItem({ meeting, selected }: { meeting: MeetingListItem; selected: boolean }) {
  const t = useT();
  const now = useNow();
  const processing = meeting.status === "transcribing" || meeting.status === "summarizing";
  const eta = processing ? etaText(t, meeting.eta, now, meeting.progress) : undefined;
  // "Summarizing · about 8 min left": the estimate continues the sentence.
  const left = eta && eta.charAt(0).toLocaleLowerCase() + eta.slice(1);
  const go = () => navigate(routes.meeting(meeting.id));
  // Screen readers hear the whole row: name, date, then what the line under the title says.
  const label = (status: string) => t("shell.meetingAria", { title: meeting.title, date: formatDate(meeting.startedAt), status });

  const base = "flex w-full cursor-pointer flex-col gap-1 rounded-xl border-0 px-3.5 py-[11px] text-left font-[inherit] text-ink";
  const tone = selected ? "bg-surface shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "bg-transparent hover:bg-hover";

  if (processing) {
    return (
      <button
        type="button"
        onClick={go}
        className={cn(base, "gap-1.5", tone)}
        aria-current={selected ? "page" : undefined}
        aria-label={label(`${t(PROCESSING_KEYS[meeting.status as keyof typeof PROCESSING_KEYS])}${left ? `, ${left}` : ""}`)}
      >
        <span className="font-semibold">{meeting.title}</span>
        <span aria-hidden="true" className="h-[3px] overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold" style={{ width: `${Math.round((meeting.progress ?? 0) * 100)}%` }} />
        </span>
        <span className={meta}>
          {t(PROCESSING_KEYS[meeting.status as keyof typeof PROCESSING_KEYS])}
          {left && ` · ${left}`}
        </span>
      </button>
    );
  }

  return (
    <button
      type="button"
      onClick={go}
      className={cn(base, tone)}
      aria-current={selected ? "page" : undefined}
      aria-label={label(
        [
          meeting.status === "recording"
            ? t("shell.statusRecording")
            : meeting.status === "failed"
              ? t("shell.statusFailed")
              : meeting.status === "needs_recovery"
                ? t("shell.statusNeedsRecovery")
                : metaLine(meeting),
          meeting.pinned ? t("shell.audioPinned") : "",
        ]
          .filter(Boolean)
          .join(", "),
      )}
    >
      <span className={cn("flex items-center gap-1.5", selected ? "font-bold" : "font-semibold")}>
        <span className="min-w-0 flex-1 truncate">{meeting.title}</span>
        {meeting.pinned && <Pin size={12} aria-label={t("shell.audioPinned")} className="flex-none" />}
      </span>
      {meeting.status === "recording" ? (
        <span className={cn(meta, "flex items-center gap-1.5 text-red")}>
          <span className="size-1.5 animate-pulse rounded-full bg-current" /> {t("shell.statusRecording")}
        </span>
      ) : meeting.status === "failed" ? (
        <span className={cn(meta, "flex items-center gap-1 text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> {t("shell.statusFailed")}
        </span>
      ) : meeting.status === "needs_recovery" ? (
        <span className={cn(meta, "flex items-center gap-1 text-warn-fg")}>
          <AlertTriangle size={12} aria-hidden="true" /> {t("shell.statusNeedsRecovery")}
        </span>
      ) : (
        <span className={meta}>{metaLine(meeting)}</span>
      )}
    </button>
  );
}
