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

/** A meeting whose meeting.json couldn't be read (damaged, or written by a newer Talaan): Rust lists it as failed
 *  with no start time (storage/store.rs `broken_item`). */
export const unreadable = (m: MeetingListItem) => m.status === "failed" && !m.startedAt;

/** The status part of a row's line, for the row and for screen readers. */
export function statusLine(m: MeetingListItem): string {
  if (unreadable(m)) return tr("shell.statusUnreadable");
  if (m.status === "recording") return tr("shell.statusRecording");
  if (m.status === "failed") return tr("shell.statusFailed");
  if (m.status === "needs_recovery") return tr("shell.statusNeedsRecovery");
  return metaLine(m);
}

/** What a screen reader hears for a finished (not processing) row: name, date, the line under it, pinned audio.
 *  An unreadable meeting has no date to say. */
export function rowLabel(m: MeetingListItem): string {
  const status = [statusLine(m), m.pinned ? tr("shell.audioPinned") : ""].filter(Boolean).join(", ");
  return m.startedAt ? tr("shell.meetingAria", { title: m.title, date: formatDate(m.startedAt), status }) : tr("shell.meetingAriaNoDate", { title: m.title, status });
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
      aria-label={rowLabel(meeting)}
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
          <AlertTriangle size={12} aria-hidden="true" /> {statusLine(meeting)}
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
