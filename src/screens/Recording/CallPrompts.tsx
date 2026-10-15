import { useState } from "react";
import { formatTime } from "../../lib/format";
import { copyText } from "../../lib/share";
import { Button, Label, cn } from "../../ui";
import { useT } from "../../lib/i18n/I18nProvider";
import { useCountdown, useElapsed } from "./useClock";
import { useSettings } from "../../data/hooks";
import { Logo } from "../../brand/Logo";
import { SunCorner } from "../../brand/SunCorner";

const card = "relative w-[420px] max-w-full overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.14)]";
const ghost = "border-transparent bg-transparent";

/** "Record this call?" prompt for apps set to Ask. `onAlways` (not for browser calls) sets the app to Always and records. */
export function Nudge({ app = "Zoom", onRecord, onDismiss, onAlways }: { app?: string; onRecord: () => void; onDismiss: () => void; onAlways?: () => void }) {
  const t = useT();
  return (
    <div role="alertdialog" aria-label={t("recording.nudgeAria")} className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex gap-4 px-[22px] py-5">
        <Logo size={40} />
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <Label>{t("recording.nudgeLabel", { app })}</Label>
          <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">{t("recording.nudgeTitle")}</span>
          <span className="text-xs leading-[17px] text-subtle">{t("recording.nudgeBody")}</span>
          <div className="mt-2.5 flex gap-2">
            <Button variant="primary" onClick={onRecord}>
              {t("recording.record")}
            </Button>
            <Button className={ghost} onClick={onDismiss}>
              {t("recording.notNow")}
            </Button>
            {onAlways && (
              <Button className={cn(ghost, "ml-auto text-subtle")} onClick={onAlways}>
                {t("recording.alwaysFor", { app })}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** Banner shown when an app set to Always starts recording automatically. */
export function AlwaysBanner({ app = "Zoom", elapsedMs, onStop, onChangeRule }: { app?: string; elapsedMs?: number; onStop: () => void; onChangeRule: () => void }) {
  const t = useT();
  const { settings } = useSettings();
  const demo = useElapsed(4000, elapsedMs === undefined);
  const elapsed = elapsedMs ?? demo;
  const [copied, setCopied] = useState(false);
  return (
    <div role="status" aria-label={t("recording.bannerAria", { app })} className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-2 px-[22px] py-5">
        <div className="flex items-center gap-2.5">
          <span className="size-[9px] flex-none animate-pulse rounded-full bg-red shadow-[0_0_0_4px_color-mix(in_srgb,var(--t-red)_20%,transparent)]" />
          <Label>{t("recording.bannerLabel", { app })}</Label>
          <span className="ml-auto font-mono text-[13px]">{formatTime(elapsed)}</span>
        </div>
        <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">{t("recording.bannerTitle")}</span>
        <span className="text-xs leading-[17px] text-subtle">{t("recording.bannerBody", { app })}</span>
        <div className="mt-2.5 flex gap-2">
          <Button
            variant="primary"
            onClick={async () => {
              await copyText(settings?.recordingNotice ?? "");
              setCopied(true);
            }}
          >
            {copied ? t("recording.copied") : t("recording.copyNotice")}
          </Button>
          <Button onClick={onStop}>{t("recording.stop")}</Button>
          <Button className={cn(ghost, "ml-auto text-subtle")} onClick={onChangeRule}>
            {t("recording.changeRule")}
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
  const t = useT();
  const [running, setRunning] = useState(true);
  const { left, reset } = useCountdown(seconds, running, () => {
    setRunning(false);
    (onTimeout ?? onStop)();
  });
  return (
    <div role="alertdialog" aria-label={t("recording.endedAria")} className={card}>
      <SunCorner size={96} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-2 px-[22px] py-5">
      <Label>
        {t("recording.endedLabel", { app: app.toLowerCase(), time: formatTime(recordedMs) })}
      </Label>
      <span className="font-display text-xl font-bold leading-6 tracking-[-0.02em]">{t("recording.endedTitle")}</span>
      <div className="mt-0.5 flex items-center gap-2.5" role="timer" aria-label={t("recording.stopsInAria", { count: left })}>
        <span aria-hidden="true" className="h-1 flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold transition-[width] duration-1000 ease-linear" style={{ width: `${(left / total) * 100}%` }} />
        </span>
        <Label>{running ? t("recording.stopsIn", { count: left }) : t("recording.stopped")}</Label>
      </div>
      <div className="mt-2.5 flex gap-2">
        <Button variant="primary" className="flex-1" onClick={() => { setRunning(false); onStop(); }}>
          {t("recording.stopNow")}
        </Button>
        <Button
          onClick={() => {
            reset();
            setRunning(true);
            onKeep();
          }}
        >
          {t("recording.keepRecording")}
        </Button>
      </div>
      </div>
    </div>
  );
}
