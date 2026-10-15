import { Pause, Play } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { formatDuration, formatTime } from "../../lib/format";
import { buildRows, lineAt, speakerName } from "../../lib/transcript";
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

const grid = "grid grid-cols-[44px_56px_minmax(0,1fr)] items-start gap-2.5 px-4 py-2";

export function TranscriptCard({ meeting, activeAt, jumpSeq, editing, playing, onTogglePlay, onJump }: Props) {
  const store = useStore();
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
    <section aria-label="Transcript" className="flex min-h-0 flex-col rounded-card border-[1.5px] border-edge bg-surface">
      <div className="flex items-center gap-2.5 border-b-[1.5px] border-line px-4 py-2.5">
        <h2 className="m-0 text-base font-bold">Transcript</h2>
        <span className="flex-1" />
        <Button
          variant="strong"
          size="icon"
          className="h-7 w-7 rounded-full"
          aria-label={playing ? "Pause audio" : "Play audio"}
          disabled={!audio || meeting.transcript.length === 0}
          title={audio ? undefined : "Audio was deleted. The transcript is kept."}
          onClick={onTogglePlay}
        >
          {playing ? <Pause size={12} fill="currentColor" aria-hidden="true" /> : <Play size={12} fill="currentColor" aria-hidden="true" />}
        </Button>
        <div aria-hidden="true" className="h-1 w-[110px] rounded-full bg-line">
          <div className="h-full rounded-full bg-ink" style={{ width: `${Math.min(100, ((activeAt ?? 0) / Math.max(1, meeting.durationMs)) * 100)}%` }} />
        </div>
        <Label>
          {formatTime(activeAt ?? 0)} / {formatTime(meeting.durationMs)}
        </Label>
      </div>

      {meeting.transcript.length > 0 && <SpeakersBar meeting={meeting} />}

      <div ref={scroller} className="flex min-h-0 flex-1 flex-col overflow-auto py-1.5">
        {busy ? (
          <div className="px-4">
            <Progress label="Transcribing the audio" value={meeting.progress} />
          </div>
        ) : rows.length === 0 ? (
          <p className="m-0 px-4 py-3 text-[13px] text-subtle">No transcript yet.</p>
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
                  <span>Paused for {formatDuration(r.pause.durationMs)} (off the record)</span>
                </div>
              );
            }
            const { line, marks } = r;
            const isActive = active?.id === line.id;
            return (
              <div
                key={line.id}
                data-line-id={line.id}
                className={cn(grid, marks.length > 0 && "bg-mark", isActive && "bg-lime-soft shadow-[inset_4px_0_0_var(--t-ink)]")}
              >
                <button
                  type="button"
                  onClick={() => onJump(line.at)}
                  aria-label={`Jump to ${formatTime(line.at)}`}
                  className="cursor-pointer border-0 bg-transparent p-0 text-left font-mono text-[11px] leading-5 text-subtle hover:text-ink"
                >
                  {formatTime(line.at)}
                </button>
                <SpeakerTag speaker={line.speaker} name={nameOf(line.speaker)} />
                <span className="[text-wrap:pretty]">
                  {marks.length > 0 && <span aria-label="Marked moment">★ </span>}
                  <Editable text={line.text} editing={editing} onCommit={(v) => store.updateTranscriptLine(meeting.id, line.id, v)} />
                  {line.edited && !editing && <Chip tone="lime" className="ml-2 align-middle">edited</Chip>}
                  {marks.filter((m) => m.note).map((m) => (
                    <span key={m.id} className="ml-3 text-[13px] text-subtle">
                      ✎ Your note: {m.note}
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
