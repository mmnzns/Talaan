import { AlertTriangle, Pause, Play, Square, Star } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useSettings } from "../../data/hooks";
import type { RecorderLevels, RecorderNotice, RecorderState } from "../../data/recorder";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Label, cn } from "../../ui";

interface Props {
  state: RecorderState;
  levels: RecorderLevels;
  notice?: RecorderNotice;
  app?: string;
  onPause: () => void;
  onResume: () => void;
  onMark: (note?: string) => void;
  onStop: () => void;
  onDiscard: () => void;
  onOpenSettings?: (pane: NonNullable<RecorderNotice["settingsPane"]>) => void;
  onDismissNotice?: () => void;
}

const BARS = 10;

/** Turns one level (0..1) into a row of bars that moves like a meter. */
function useBars(level: number, active: boolean, frame: unknown) {
  const history = useRef<number[]>(Array(BARS).fill(0));
  const [bars, setBars] = useState<number[]>(history.current);
  useEffect(() => {
    history.current = active ? [level, ...history.current.slice(0, BARS - 1)] : Array(BARS).fill(0);
    setBars([...history.current]);
    // `frame` changes with every levels event, so a steady level (e.g. silence) still scrolls the meter.
  }, [level, active, frame]);
  return bars;
}

function Bars({ levels, tone }: { levels: number[]; tone: "me" | "them" }) {
  return (
    <div aria-hidden="true" className="flex h-6 items-center gap-1">
      {levels.map((l, i) => (
        <span
          key={i}
          className={cn("w-[5px] rounded-[3px] transition-[height] duration-150", l > 0.05 ? (tone === "me" ? "bg-ink" : "bg-input") : "bg-line")}
          style={{ height: l > 0.05 ? 6 + Math.round(l * 18) : 4 }}
        />
      ))}
    </div>
  );
}

/** Menu bar popover while recording: notice, levels, pause, mark, quick note, stop. */
export function RecordingPopover({ state, levels, notice, app = "Call", onPause, onResume, onMark, onStop, onDiscard, onOpenSettings, onDismissNotice }: Props) {
  const { settings } = useSettings();
  const [copied, setCopied] = useState(false);
  const [note, setNote] = useState("");
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const paused = state.state === "paused";
  const me = useBars(levels.me, !paused, levels);
  const them = useBars(levels.them, !paused, levels);

  const text = settings?.recordingNotice ?? "";
  const info = [
    paused ? "Paused: nothing is being recorded" : null,
    `${state.marks} ${state.marks === 1 ? "mark" : "marks"}`,
    `${state.notes} ${state.notes === 1 ? "note" : "notes"} so far`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div role="dialog" aria-label="Recording" className="flex w-[356px] flex-col overflow-hidden rounded-card border border-edge bg-surface shadow-pop">
      <div className="flex items-center gap-2.5 bg-aside p-3.5 text-aside-fg">
        <span className={cn("size-2.5 rounded-full", paused ? "bg-aside-muted" : "bg-gold shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-gold)_30%,transparent)]")} />
        <span className="font-bold">{paused ? "Paused" : "Recording"}</span>
        <span className="font-mono text-[11px] text-aside-muted">{app}</span>
        <span className="ml-auto font-mono text-xl font-semibold text-gold" aria-label="Elapsed time">
          {formatTime(state.elapsedMs)}
        </span>
      </div>

      <div className="flex flex-col gap-3.5 p-3.5">
        <div className="flex flex-col gap-2">
          <span className="font-bold">Let people know you’re recording</span>
          <div className="rounded-control border border-line bg-paper px-3 py-2.5 text-[13px] leading-[18px]">{text}</div>
          <Button
            raised={!copied}
            className={cn("h-[34px]", copied && "bg-gold-soft")}
            onClick={async () => {
              await copyText(text);
              setCopied(true);
            }}
          >
            {copied ? "Copied. Paste it in the meeting chat" : "Copy notice"}
          </Button>
        </div>

        {state.onBuiltInSpeakers && settings?.warnOnSpeakers !== false && (
          <div role="note" className="flex items-start gap-2.5 rounded-control border border-warn-fg bg-warn-bg px-3 py-2.5 text-warn-fg">
            <AlertTriangle size={16} aria-hidden="true" className="mt-px flex-none" />
            <span className="text-xs leading-[17px]">You’re on the MacBook speakers. Use headphones so other people’s voices aren’t recorded twice.</span>
          </div>
        )}

        {notice && (
          <div
            role="status"
            className={cn(
              "flex flex-col gap-2 rounded-control border px-3 py-2 text-xs leading-[17px]",
              notice.kind === "warning" ? "border-warn-fg bg-warn-bg text-warn-fg" : "border-line bg-paper",
            )}
          >
            <span>{notice.text}</span>
            <span className="flex gap-2">
              {notice.settingsPane && onOpenSettings && (
                <Button className="h-7" onClick={() => onOpenSettings(notice.settingsPane!)}>
                  Open System Settings
                </Button>
              )}
              {onDismissNotice && (
                <Button variant="ghost" className="h-7" onClick={onDismissNotice}>
                  OK
                </Button>
              )}
            </span>
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
          <Label>
            Mic: {state.micName || "none"} · {state.systemAudio ? "saved every few seconds" : "call audio not available"}
          </Label>
        </div>

        <div className="flex flex-col gap-2 border-t border-line pt-3">
          <div className="flex gap-2">
            <Button size="lg" variant={paused ? "strong" : "default"} raised={!paused} onClick={paused ? onResume : onPause}>
              {paused ? <Play size={13} fill="currentColor" aria-hidden="true" /> : <Pause size={13} fill="currentColor" aria-hidden="true" />}
              {paused ? "Resume" : "Pause"}
            </Button>
            <Button size="lg" raised className="flex-1" disabled={paused} onClick={() => onMark()}>
              <Star size={13} aria-hidden="true" /> Mark this moment
            </Button>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (note.trim()) {
                onMark(note.trim());
                setNote("");
              }
            }}
          >
            <label className="flex h-[34px] items-center gap-2 rounded-control border border-input bg-paper px-2.5 focus-within:border-edge">
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
