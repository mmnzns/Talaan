import { Pause, Play } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { formatDuration, formatTime } from "../../lib/format";
import { buildRows, lineAt, speakerName } from "../../lib/transcript";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Chip, Editable, Label, SpeakerTag, cn } from "../../ui";
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
  const rows = useMemo(() => buildRows(meeting.transcript, meeting.pauses, meeting.marks), [meeting.transcript, meeting.pauses, meeting.marks]);
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

  return (
    <section aria-label={t("meeting.transcript")} className="flex min-h-0 flex-col overflow-hidden rounded-card border border-line bg-surface shadow-card">
      <div className="flex items-center gap-3 border-b border-line px-6 py-4">
        <h2 className="m-0 font-display text-base font-bold tracking-[-0.02em]">{t("meeting.transcript")}</h2>
        <span className="flex-1" />
        <Button
          variant="strong"
          size="icon"
          className="h-7 w-7 rounded-full"
          aria-label={playing ? t("meeting.pauseAudio") : t("meeting.playAudio")}
          disabled={!audio || meeting.transcript.length === 0}
          title={audio ? undefined : t("meeting.audioGoneTip")}
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
      </div>

      {meeting.transcript.length > 0 && <SpeakersBar meeting={meeting} />}

      <div ref={scroller} className="flex min-h-0 flex-1 flex-col overflow-auto pb-3.5 pt-2.5">
        {busy ? (
          <div className="px-4">
            <Progress label={t("meeting.transcribingAudio")} value={meeting.progress} />
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
                  <Editable text={line.text} editing={editing} onCommit={(v) => store.updateTranscriptLine(meeting.id, line.id, v)} />
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
