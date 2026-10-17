import { ChevronDown, ChevronUp, Pause, Play, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { formatDuration, formatTime } from "../../lib/format";
import { findMatches, segments } from "../../lib/find";
import { buildRows, lineAt, speakerName } from "../../lib/transcript";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Chip, Editable, Field, Label, SpeakerTag, cn } from "../../ui";
import { Progress } from "./SummaryCard";
import { SpeakersBar } from "./SpeakersBar";

interface Props {
  meeting: Meeting;
  activeAt?: Ms;
  jumpSeq: number;
  editing: boolean;
  playing: boolean;
  onTogglePlay: () => void;
  onJump: (at: Ms) => void;
}

const grid = "grid grid-cols-[44px_56px_minmax(0,1fr)] items-start gap-3.5 px-6 py-3";

export function TranscriptCard({ meeting, activeAt, jumpSeq, editing, playing, onTogglePlay, onJump }: Props) {
  const store = useStore();
  const t = useT();
  // Notes added after the call have no moment, so they stay out of the transcript.
  const marks = useMemo(() => meeting.marks.filter((m) => !m.later), [meeting.marks]);
  const rows = useMemo(() => buildRows(meeting.transcript, meeting.pauses, marks), [meeting.transcript, meeting.pauses, marks]);
  const active = activeAt === undefined ? undefined : lineAt(meeting.transcript, activeAt);
  const scroller = useRef<HTMLDivElement>(null);
  const audio = meeting.hasAudio;

  useEffect(() => {
    if (!active) return;
    const el = scroller.current?.querySelector<HTMLElement>(`[data-line-id="${active.id}"]`);
    el?.scrollIntoView({ block: playing ? "nearest" : "center", behavior: "smooth" });
  }, [active?.id, jumpSeq]);

  const busy = meeting.status === "transcribing";
  const nameOf = (s: string) => speakerName(meeting, s);

  // Find in transcript: every match highlighted, one current, stepped with the arrows or Enter / Shift+Enter.
  const [query, setQuery] = useState("");
  const [picked, setCurrent] = useState(0);
  const ordered = useMemo(() => buildRows(meeting.transcript, [], []).flatMap((r) => (r.kind === "line" ? [r.line] : [])), [meeting.transcript]);
  const matches = useMemo(() => findMatches(ordered, query).map((m, index) => ({ ...m, index })), [ordered, query]);
  const byLine = useMemo(() => {
    const map = new Map<string, typeof matches>();
    for (const m of matches) map.set(m.lineId, [...(map.get(m.lineId) ?? []), m]);
    return map;
  }, [matches]);
  const current = Math.max(0, Math.min(picked, matches.length - 1));
  const step = (by: number) => matches.length && setCurrent((c) => (c + by + matches.length) % matches.length);
  useEffect(() => setCurrent(0), [query]);
  useEffect(() => {
    if (!matches.length) return;
    scroller.current?.querySelector<HTMLElement>(`[data-match="${current}"]`)?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [current, matches]);

  return (
    <section aria-label={t("meeting.transcript")} className="flex min-h-0 flex-col overflow-hidden rounded-card border border-line bg-surface shadow-card">
      <div className="flex items-center gap-3 border-b border-line px-6 py-4">
        <h2 className="m-0 font-display text-base font-bold tracking-[-0.02em]">{t("meeting.transcript")}</h2>
        <span className="flex-1" />
        {audio ? (
          <>
            <Button
              variant="strong"
              size="icon"
              className="h-7 w-7 rounded-full"
              aria-label={playing ? t("meeting.pauseAudio") : t("meeting.playAudio")}
              disabled={meeting.transcript.length === 0}
              onClick={onTogglePlay}
            >
              {playing ? <Pause size={11} fill="currentColor" aria-hidden="true" className="text-gold" /> : <Play size={11} fill="currentColor" aria-hidden="true" className="text-gold" />}
            </Button>
            <div aria-hidden="true" className="h-[3px] w-[110px] rounded-full bg-line">
              <div className="h-full rounded-full bg-gold" style={{ width: `${Math.min(100, ((activeAt ?? 0) / Math.max(1, meeting.durationMs)) * 100)}%` }} />
            </div>
            <Label>
              {formatTime(activeAt ?? 0)} / {formatTime(meeting.durationMs)}
            </Label>
          </>
        ) : (
          // No audio to play (deleted by the retention setting or by hand): no player, just say so.
          <Chip title={t("meeting.audioGoneTip")}>{t("meeting.audioDeletedChip")}</Chip>
        )}
      </div>

      {meeting.transcript.length > 0 && !busy && (
        <div className="flex items-center gap-2 border-b border-line px-6 py-2">
          <Field
            label={t("meeting.find")}
            icon={<Search size={13} aria-hidden="true" />}
            placeholder={t("meeting.find")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
              if (e.key === "Escape") setQuery("");
            }}
            wrapperClassName="h-7 min-w-0 flex-1 px-2.5"
          />
          {query.trim() && (
            <>
              <Label aria-live="polite">{matches.length ? t("meeting.findCount", { n: current + 1, total: matches.length }) : t("meeting.findNone")}</Label>
              <Button size="icon" className="h-7 w-7" aria-label={t("meeting.findPrev")} disabled={!matches.length} onClick={() => step(-1)}>
                <ChevronUp size={14} aria-hidden="true" />
              </Button>
              <Button size="icon" className="h-7 w-7" aria-label={t("meeting.findNext")} disabled={!matches.length} onClick={() => step(1)}>
                <ChevronDown size={14} aria-hidden="true" />
              </Button>
              <Button size="icon" variant="ghost" className="h-7 w-7" aria-label={t("meeting.findClear")} onClick={() => setQuery("")}>
                <X size={14} aria-hidden="true" />
              </Button>
            </>
          )}
        </div>
      )}

      {meeting.transcript.length > 0 && <SpeakersBar meeting={meeting} />}

      <div ref={scroller} className="flex min-h-0 flex-1 flex-col overflow-auto pb-3.5 pt-2.5">
        {busy ? (
          <div className="px-4">
            <Progress label={t("meeting.transcribingAudio")} value={meeting.progress} eta={meeting.eta} />
          </div>
        ) : rows.length === 0 ? (
          <p className="m-0 px-4 py-3 text-[13px] text-subtle">{t("meeting.noTranscript")}</p>
        ) : (
          rows.map((r) => {
            if (r.kind === "pause") {
              return (
                <div
                  key={`pause-${r.pause.at}`}
                  className={cn(grid, "bg-[repeating-linear-gradient(135deg,var(--t-paper)_0_6px,var(--t-rail)_6px_12px)] text-[13px] italic text-subtle")}
                >
                  <span className="font-mono text-[11px] leading-5 not-italic">{formatTime(r.pause.at)}</span>
                  <span />
                  <span>{t("meeting.pausedFor", { duration: formatDuration(r.pause.durationMs) })}</span>
                </div>
              );
            }
            const { line, marks } = r;
            const isActive = active?.id === line.id;
            return (
              <div
                key={line.id}
                data-line-id={line.id}
                className={cn(grid, marks.length > 0 && "bg-mark", isActive && "bg-gold-soft shadow-[inset_3px_0_0_var(--t-gold)]")}
              >
                <button
                  type="button"
                  onClick={() => onJump(line.at)}
                  aria-label={t("meeting.jumpTo", { time: formatTime(line.at) })}
                  className="cursor-pointer border-0 bg-transparent p-0 text-left font-mono text-[11px] leading-5 text-faint hover:text-ink"
                >
                  {formatTime(line.at)}
                </button>
                <SpeakerTag speaker={line.speaker} name={nameOf(line.speaker)} />
                <span className="[text-wrap:pretty]">
                  {marks.length > 0 && <span aria-label={t("meeting.markedMoment")}>★ </span>}
                  {byLine.has(line.id) && !editing ? (
                    segments(line.text, byLine.get(line.id)!).map((seg, i) =>
                      seg.match === undefined ? (
                        <span key={i}>{seg.text}</span>
                      ) : (
                        <mark key={i} data-match={seg.match} className={cn("rounded-[3px] px-px text-ink", seg.match === current ? "bg-gold" : "bg-gold-soft")}>
                          {seg.text}
                        </mark>
                      ),
                    )
                  ) : (
                    <Editable text={line.text} editing={editing} onCommit={(v) => store.updateTranscriptLine(meeting.id, line.id, v)} />
                  )}
                  {line.edited && !editing && <Chip tone="gold" className="ml-2 align-middle">{t("meeting.editedChip")}</Chip>}
                  {marks.filter((m) => m.note).map((m) => (
                    <span key={m.id} className="ml-3 text-[13px] text-subtle">
                      {t("meeting.yourNote", { note: m.note ?? "" })}
                    </span>
                  ))}
                </span>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}
