import { useState } from "react";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Checkbox, Label, cn } from "../../ui";
import { useCountdown, useElapsed } from "./useClock";
import { useSettings } from "../../data/hooks";
import { Logo } from "../../brand/Logo";
import { SunCorner } from "../../brand/SunCorner";

const card = "relative w-[420px] max-w-full overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.14)]";
const ghost = "border-transparent bg-transparent";

/** "Record this call?" prompt for apps set to Ask. `onAlways` (not for browser calls) sets the app to Always and records. */
export function Nudge({
  app = "Zoom",
  onRecord,
  onDismiss,
  onAlways,
}: {
  app?: string;
  /** `transcriptOnly`: save this call without a summary until the user asks for one. */
  onRecord: (transcriptOnly: boolean) => void;
  onDismiss: () => void;
  onAlways?: () => void;
}) {
  const [transcriptOnly, setTranscriptOnly] = useState(false);
  return (
    <div role="alertdialog" aria-label="Record this call?" className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex gap-4 px-[22px] py-5">
        <Logo size={40} />
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <Label>{app} is using your mic</Label>
          <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">Record this call?</span>
          <span className="text-xs leading-[17px] text-subtle">Stays on this computer. You’ll get a notice to paste in the chat.</span>
          <div className="mt-2.5 flex gap-2">
            <Button variant="primary" onClick={() => onRecord(transcriptOnly)}>
              Record
            </Button>
            <Button className={ghost} onClick={onDismiss}>
              Not now
            </Button>
            {onAlways && (
              <Button className={cn(ghost, "ml-auto text-subtle")} onClick={onAlways}>
                Always for {app}
              </Button>
            )}
          </div>
          <label className="mt-1 flex cursor-pointer items-center gap-2 text-xs text-subtle">
            <Checkbox aria-label="Transcript only, no summary" checked={transcriptOnly} onChange={setTranscriptOnly} className="mt-0" />
            Transcript only, no summary
          </label>
        </div>
      </div>
    </div>
  );
}

/** Banner shown when an app set to Always starts recording automatically. */
export function AlwaysBanner({ app = "Zoom", elapsedMs, onStop, onChangeRule }: { app?: string; elapsedMs?: number; onStop: () => void; onChangeRule: () => void }) {
  const { settings } = useSettings();
  const demo = useElapsed(4000, elapsedMs === undefined);
  const elapsed = elapsedMs ?? demo;
  const [copied, setCopied] = useState(false);
  return (
    <div role="status" aria-label={`Recording ${app} automatically`} className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-2 px-[22px] py-5">
        <div className="flex items-center gap-2.5">
          <span className="size-[9px] flex-none animate-pulse rounded-full bg-red shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-red)_20%,transparent)]" />
          <Label>recording {app} automatically</Label>
          <span className="ml-auto font-mono text-[13px]">{formatTime(elapsed)}</span>
        </div>
        <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">Let people know</span>
        <span className="text-xs leading-[17px] text-subtle">{app} is set to Always. Paste the notice in the call chat.</span>
        <div className="mt-2.5 flex gap-2">
          <Button
            variant="primary"
            onClick={async () => {
              await copyText(settings?.recordingNotice ?? "");
              setCopied(true);
            }}
          >
            {copied ? "Copied" : "Copy notice"}
          </Button>
          <Button onClick={onStop}>Stop</Button>
          <Button className={cn(ghost, "ml-auto text-subtle")} onClick={onChangeRule}>
            Change rule
          </Button>
        </div>
      </div>
    </div>
  );
}

/** "Call ended. Stop recording?" with a 30 second auto-stop. */
export function CallEndedPrompt({
  app = "Zoom",
  recordedMs = 42 * 60_000 + 10_000,
  seconds = 30,
  total = 30,
  onStop,
  onKeep,
  onTimeout,
}: {
  app?: string;
  recordedMs?: number;
  /** Seconds left on the countdown. */
  seconds?: number;
  /** Length of the whole countdown, for the bar. */
  total?: number;
  onStop: () => void;
  onKeep: () => void;
  /** What happens at zero; defaults to `onStop`. (In the app Talaan stops by itself, so the window just waits.) */
  onTimeout?: () => void;
}) {
  const [running, setRunning] = useState(true);
  const { left, reset } = useCountdown(seconds, running, () => {
    setRunning(false);
    (onTimeout ?? onStop)();
  });
  return (
    <div role="alertdialog" aria-label="Call ended" className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-2 px-[22px] py-5">
      <Label>
        {app.toLowerCase()} stopped using the mic · {formatTime(recordedMs)} recorded
      </Label>
      <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">Call ended. Stop recording?</span>
      <div className="mt-0.5 flex items-center gap-2.5" role="timer" aria-label={`Stops in ${left} seconds`}>
        <span aria-hidden="true" className="h-1 flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold transition-[width] duration-1000 ease-linear" style={{ width: `${(left / total) * 100}%` }} />
        </span>
        <Label>{running ? `stops in ${left}s` : "stopped"}</Label>
      </div>
      <div className="mt-2.5 flex gap-2">
        <Button variant="primary" className="flex-1" onClick={() => { setRunning(false); onStop(); }}>
          Stop and save now
        </Button>
        <Button
          onClick={() => {
            reset();
            setRunning(true);
            onKeep();
          }}
        >
          Keep recording
        </Button>
      </div>
      </div>
    </div>
  );
}
