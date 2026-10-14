import { AlertTriangle, Pause, Play, Square, Star } from "lucide-react";
import { useEffect, useState } from "react";
import { useSettings } from "../../data/hooks";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Label, cn } from "../../ui";
import { useElapsed } from "./useClock";

interface Props {
  app?: string;
  startAtMs?: number;
  /** Output is the built-in speakers, so the headphones warning shows. */
  onBuiltInSpeakers?: boolean;
  /** Small notice such as a device switch. */
  notice?: string;
  onStop: () => void;
  onDiscard: () => void;
}

const BARS = 10;

function useLevels(active: boolean, loud: number) {
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  useEffect(() => {
    if (!active) {
      setLevels(Array(BARS).fill(0));
      return;
    }
    const timer = setInterval(() => {
      setLevels(Array.from({ length: BARS }, (_, i) => (Math.random() < 0.15 + loud * (1 - i / BARS) ? Math.random() : 0)));
    }, 180);
    return () => clearInterval(timer);
  }, [active, loud]);
  return levels;
}

function Bars({ levels, tone }: { levels: number[]; tone: "me" | "them" }) {
  return (
    <div aria-hidden="true" className="flex h-6 items-center gap-1">
      {levels.map((l, i) => (
        <span
          key={i}
          className={cn("w-[5px] rounded-[3px] transition-[height] duration-150", l > 0 ? (tone === "me" ? "bg-ink" : "bg-input") : "bg-line")}
          style={{ height: l > 0 ? 6 + Math.round(l * 18) : 4 }}
        />
      ))}
    </div>
  );
}

/** Menu bar popover while recording: notice, levels, pause, mark, quick note, stop. */
export function RecordingPopover({ app = "Zoom", startAtMs = 0, onBuiltInSpeakers = true, notice, onStop, onDiscard }: Props) {
  const { settings } = useSettings();
  const [paused, setPaused] = useState(false);
  const [copied, setCopied] = useState(false);
  const [marks, setMarks] = useState(0);
  const [notes, setNotes] = useState(0);
  const [note, setNote] = useState("");
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const elapsed = useElapsed(startAtMs, !paused);
  const me = useLevels(!paused, 0.5);
  const them = useLevels(!paused, 0.7);

  const text = settings?.recordingNotice ?? "";

  const info = [
    paused ? "Paused: nothing is being recorded" : null,
    `${marks} ${marks === 1 ? "mark" : "marks"}`,
    `${notes} ${notes === 1 ? "note" : "notes"} so far`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div role="dialog" aria-label="Recording" className="flex w-[356px] flex-col overflow-hidden rounded-card border-[1.5px] border-edge bg-surface shadow-hard-lg">
      <div className="flex items-center gap-2.5 bg-aside p-3.5 text-aside-fg">
        <span
          className={cn(
            "size-2.5 rounded-full",
            paused ? "bg-aside-muted" : "bg-lime shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-lime)_30%,transparent)]",
          )}
        />
        <span className="font-bold">{paused ? "Paused" : "Recording"}</span>
        <span className="font-mono text-[11px] text-aside-muted">{app}</span>
        <span className="ml-auto font-mono text-xl font-semibold text-lime" aria-label="Elapsed time">
          {formatTime(elapsed)}
        </span>
      </div>

      <div className="flex flex-col gap-3.5 p-3.5">
        <div className="flex flex-col gap-2">
          <span className="font-bold">Let people know you’re recording</span>
          <div className="rounded-control border-[1.5px] border-line bg-paper px-3 py-2.5 text-[13px] leading-[18px]">{text}</div>
          <Button
            raised={!copied}
            className={cn("h-[34px]", copied && "bg-lime-soft")}
            onClick={async () => {
              await copyText(text);
              setCopied(true);
            }}
          >
            {copied ? "Copied. Paste it in the meeting chat" : "Copy notice"}
          </Button>
        </div>

        {onBuiltInSpeakers && settings?.warnOnSpeakers !== false && (
          <div role="note" className="flex items-start gap-2.5 rounded-control border-[1.5px] border-warn-fg bg-warn-bg px-3 py-2.5 text-warn-fg">
            <AlertTriangle size={16} aria-hidden="true" className="mt-px flex-none" />
            <span className="text-xs leading-[17px]">You’re on the MacBook speakers. Use headphones so other people’s voices aren’t recorded twice.</span>
          </div>
        )}

        {notice && (
          <div role="status" className="rounded-control border border-line bg-paper px-3 py-2 text-xs">
            {notice}
          </div>
        )}

        <div className="flex flex-col gap-2">
          <div className="grid grid-cols-[48px_minmax(0,1fr)] items-center gap-2.5">
            <span className="rounded-full bg-strong py-[3px] text-center font-mono text-[11px] font-semibold text-strong-fg">Me</span>
            <Bars levels={me} tone="me" />
          </div>
          <div className="grid grid-cols-[48px_minmax(0,1fr)] items-center gap-2.5">
            <span className="rounded-full border border-line py-0.5 text-center font-mono text-[11px] font-semibold text-subtle">Them</span>
            <Bars levels={them} tone="them" />
          </div>
          <Label>Mic: AirPods Pro · saved every few seconds</Label>
        </div>

        <div className="flex flex-col gap-2 border-t-[1.5px] border-line pt-3">
          <div className="flex gap-2">
            <Button size="lg" variant={paused ? "strong" : "default"} raised={!paused} onClick={() => setPaused((p) => !p)}>
              {paused ? <Play size={13} fill="currentColor" aria-hidden="true" /> : <Pause size={13} fill="currentColor" aria-hidden="true" />}
              {paused ? "Resume" : "Pause"}
            </Button>
            <Button size="lg" raised className="flex-1" disabled={paused} onClick={() => setMarks((n) => n + 1)}>
              <Star size={13} aria-hidden="true" /> Mark this moment
            </Button>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (note.trim()) {
                setNotes((n) => n + 1);
                setNote("");
              }
            }}
          >
            <label className="flex h-[34px] items-center gap-2 rounded-control border-[1.5px] border-input bg-paper px-2.5 focus-within:border-edge">
              <span className="sr-only">Quick note</span>
              <input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                disabled={paused}
                placeholder="Quick note, then Enter"
                className="min-w-0 flex-1 border-0 bg-transparent font-[inherit] text-[13px] text-ink outline-none placeholder:text-subtle"
              />
            </label>
          </form>
          <Label>{info}</Label>
        </div>

        <div className="mt-1 flex gap-2.5">
          <Button variant="primary" raised className="h-10 flex-1" onClick={onStop}>
            <Square size={13} fill="currentColor" aria-hidden="true" /> Stop and save
          </Button>
          <Button
            variant="ghost"
            className="h-10 text-warn-fg hover:bg-warn-bg"
            onClick={() => (confirmDiscard ? onDiscard() : setConfirmDiscard(true))}
            onBlur={() => setConfirmDiscard(false)}
          >
            {confirmDiscard ? "Really discard?" : "Discard"}
          </Button>
        </div>
      </div>
    </div>
  );
}
