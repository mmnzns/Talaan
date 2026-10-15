import { Mic } from "lucide-react";
import { useState } from "react";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Label, cn } from "../../ui";
import { useCountdown, useElapsed } from "./useClock";
import { useSettings } from "../../data/hooks";

const card = "w-[420px] max-w-full rounded-card border border-edge bg-surface shadow-pop";

/** "Record this call?" prompt for apps set to Ask. */
export function Nudge({ app = "Zoom", onRecord, onDismiss }: { app?: string; onRecord: () => void; onDismiss: () => void }) {
  return (
    <div role="alertdialog" aria-label="Record this call?" className={cn(card, "flex gap-3.5 p-4")}>
      <span aria-hidden="true" className="grid size-10 flex-none place-items-center rounded-control border border-edge bg-gold text-on-gold">
        <Mic size={20} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <Label>{app.toLowerCase()} is using your mic</Label>
        <span className="text-[17px] font-extrabold leading-[22px] tracking-[-0.02em]">Record this call?</span>
        <span className="text-xs leading-4 text-subtle">Stays on this computer. You’ll get a notice to paste in the chat.</span>
        <div className="mt-2 flex gap-2">
          <Button variant="primary" raised onClick={onRecord}>
            Record
          </Button>
          <Button variant="ghost" onClick={onDismiss}>
            Not now
          </Button>
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
    <div
      role="status"
      aria-label={`Recording ${app} automatically`}
      className="flex w-[420px] max-w-full flex-col gap-2.5 rounded-card border border-edge bg-aside px-4 py-3.5 text-aside-fg shadow-[5px_5px_0_var(--t-gold)]"
    >
      <div className="flex items-center gap-2.5">
        <span className="size-2.5 rounded-full bg-gold shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-gold)_30%,transparent)]" />
        <span className="text-[15px] font-extrabold">Recording {app} automatically</span>
        <span className="ml-auto font-mono text-[13px] text-gold">{formatTime(elapsed)}</span>
      </div>
      <span className="text-xs leading-4 text-aside-muted">{app} is set to Always. Let people know with the notice.</span>
      <div className="flex gap-2">
        <Button
          size="lg"
          className={cn("border-gold", copied ? "bg-gold-soft text-ink" : "bg-gold text-on-gold hover:bg-gold")}
          onClick={async () => {
            await copyText(settings?.recordingNotice ?? "");
            setCopied(true);
          }}
        >
          {copied ? "Copied" : "Copy notice"}
        </Button>
        <Button size="lg" variant="ghost" className="border-aside-muted text-aside-fg hover:bg-white/10" onClick={onStop}>
          Stop
        </Button>
        <Button size="lg" variant="ghost" className="ml-auto text-aside-muted hover:bg-white/10 hover:text-aside-fg" onClick={onChangeRule}>
          Change rule
        </Button>
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
    <div role="alertdialog" aria-label="Call ended" className={cn(card, "flex flex-col gap-2 px-4 py-3.5")}>
      <Label>
        {app.toLowerCase()} stopped using the mic · {formatTime(recordedMs)} recorded
      </Label>
      <span className="text-[17px] font-extrabold tracking-[-0.02em]">Call ended. Stop recording?</span>
      <div className="flex items-center gap-2" role="timer" aria-label={`Stops in ${left} seconds`}>
        <span aria-hidden="true" className="h-[5px] flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-ink transition-[width] duration-1000 ease-linear" style={{ width: `${(left / total) * 100}%` }} />
        </span>
        <Label>{running ? `stops in ${left}s` : "stopped"}</Label>
      </div>
      <div className="mt-0.5 flex gap-2">
        <Button variant="primary" raised className="flex-1" onClick={() => { setRunning(false); onStop(); }}>
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
  );
}
