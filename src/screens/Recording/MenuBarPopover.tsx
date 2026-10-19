import { useRef, useState } from "react";
import { useMeetings, useNextMeeting } from "../../data/hooks";
import type { NextMeeting } from "../../data/settings";
import { startsIn } from "../../lib/nextMeeting";
import { useStore } from "../../data/StoreProvider";
import { dayGroup, formatClock } from "../../lib/format";
import { copyText, toShareText } from "../../lib/share";
import { openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, Label, useToast } from "../../ui";
import { useT } from "../../lib/i18n/I18nProvider";
import { Logo } from "../../brand/Logo";
import { etaText, PROCESSING_KEYS, useNow } from "../../lib/eta";
import { SunCorner } from "../../brand/SunCorner";
import { liveText, phaseText, useUpdate } from "../../data/updates";

interface Props {
  /** `next`: the calendar meeting to record (its title and attendees go on the recording). */
  onStart: (next?: NextMeeting) => void;
  /** The next meeting to show; when left out, it comes from the calendar (`useNextMeeting`). */
  next?: NextMeeting | null;
  /** Open a meeting in the main window (the popover is its own window in the app). */
  onOpenMeeting?: (id: string) => void;
  onOpenApp?: () => void;
  onQuit?: () => void;
  /** A recording is running: Update now is off (the tray shows the recording popover then, so this is a guard). */
  recording?: boolean;
}

/** Menu bar popover when nothing is recording. */
export function MenuBarPopover({ onStart, onOpenMeeting, onOpenApp, onQuit, next: given, recording = false }: Props) {
  const t = useT();
  const store = useStore();
  const toast = useToast();
  const { meetings } = useMeetings();
  const [copied, setCopied] = useState<string>();
  const recent = meetings.slice(0, 3);
  const now = useNow();
  const fetched = useNextMeeting();
  const candidate = given !== undefined ? given : fetched;
  const next = candidate && startsIn(candidate, now) !== "ended" ? candidate : undefined;

  const copy = async (id: string) => {
    const m = await store.getMeeting(id);
    if (!m) return;
    await copyText(toShareText(m));
    setCopied(id);
    setTimeout(() => setCopied(undefined), 1800);
  };

  const item = "flex items-center gap-3 rounded-[10px] px-3 py-2.5 hover:bg-hover";

  return (
    <div role="dialog" aria-label={t("recording.menuAria")} className="flex w-[356px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.14)]">
      <div className="relative flex flex-col gap-[18px] overflow-hidden border-b border-line px-[22px] pb-5 pt-[22px]">
        <SunCorner size={110} className="absolute right-0 top-0" />
        <div className="relative flex items-center gap-2.5">
          <Logo size={34} />
          <div className="flex flex-col">
            <span className="font-display text-[17px] font-bold leading-5 tracking-[-0.02em]">{t("recording.wordmark")}</span>
            <Label className="inline-flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-gold" />
              {t("recording.readyOffline")}
            </Label>
          </div>
        </div>
        {next ? (
          <NextMeetingCard next={next} now={now} onStart={() => onStart(next)} onOther={() => onStart()} />
        ) : (
          <Button variant="primary" className="relative h-[42px] w-full text-sm" onClick={() => onStart()}>
            <RecDot />
            {t("recording.startRecording")}
          </Button>
        )}
        <span className="relative text-xs leading-[17px] text-subtle">{t("recording.asksWhen")}</span>
      </div>
      <div className="flex flex-col gap-1 px-2.5 py-4">
        <Label className="px-3 pb-2">{t("recording.recent")}</Label>
        {recent.length === 0 && <span className="px-2.5 py-2 text-[13px] text-subtle">{t("recording.recentEmpty")}</span>}
        {recent.map((m) => {
          const busy = m.status === "summarizing" || m.status === "transcribing";
          // Lowercase like the rest of this mono line ("summarizing · about 7 min left").
          const left = busy ? etaText(t, m.eta, now, m.progress)?.toLocaleLowerCase() : undefined;
          const busyText = !busy
            ? ""
            : left
              ? t(m.status === "summarizing" ? "recording.busySummarizing" : "recording.busyTranscribing", { eta: left })
              : t(PROCESSING_KEYS[m.status as keyof typeof PROCESSING_KEYS]);
          const day = dayGroup(m.startedAt).label;
          return (
            <div key={m.id} className={item}>
              <button
                type="button"
                onClick={() => (onOpenMeeting ? onOpenMeeting(m.id) : navigate(routes.meeting(m.id)))}
                className="flex min-w-0 flex-1 cursor-pointer flex-col border-0 bg-transparent p-0 text-left font-[inherit] text-ink"
              >
                <span className="truncate font-semibold">{m.title}</span>
                <Label>{busy ? busyText : `${day} ${formatClock(m.startedAt)}`}</Label>
              </button>
              {m.status === "ready" && (
                <button
                  type="button"
                  onClick={() => copy(m.id)}
                  className="h-[26px] cursor-pointer whitespace-nowrap rounded-[7px] border border-edge bg-surface px-2.5 font-[inherit] text-xs font-semibold text-ink hover:bg-hover"
                >
                  {copied === m.id ? t("recording.copied") : t("recording.copyAsText")}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <UpdateLine recording={recording} />
      <div className="flex border-t border-line p-2">
        <button type="button" onClick={() => (onOpenApp ? onOpenApp() : navigate(routes.meetings()))} className={`${item} flex-1 cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] font-semibold text-ink`}>
          {t("recording.openTalaan")}
        </button>
        <button type="button" onClick={() => openSettings()} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          {t("recording.settings")}
        </button>
        <button type="button" onClick={() => (onQuit ? onQuit() : toast(t("recording.quitDemo")))} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          {t("recording.quit")}
        </button>
      </div>
    </div>
  );
}

/** "Talaan {version} is ready" with Update now, for people who only open the menu bar / tray. */
function UpdateLine({ recording }: { recording: boolean }) {
  const t = useT();
  const u = useUpdate();
  const line = useRef<HTMLDivElement>(null);
  if (!u.pending) return null;
  const busy = u.phase.state === "installing" || u.phase.state === "restarting";
  const progress = phaseText(t, u.phase);
  const title = t("shell.updateBannerTitle", { version: u.pending.version });
  const install = () => {
    // The button gives way to the progress line: keep the focus here.
    line.current?.focus();
    void u.install();
  };
  return (
    <div ref={line} tabIndex={-1} className="mx-2.5 mb-2.5 flex items-center gap-2.5 rounded-[10px] bg-gold-soft px-3 py-2 outline-none">
      <span aria-hidden="true" className="size-1.5 flex-none rounded-full bg-gold" />
      {/* Read out at the start, every 25%, Restarting and a failure; not at every percent. */}
      <span role="status" aria-live="polite" className="sr-only">
        {liveText(t, u.phase) ?? title}
      </span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[13px] font-semibold text-ink">{title}</span>
        {(progress || recording) && (
          <span id="update-line-detail" aria-hidden={busy || undefined} className={`text-xs leading-[16px] ${u.phase.state === "error" ? "text-warn-fg" : "text-subtle"}`}>
            {progress ?? t("shell.updateWhileRecording")}
          </span>
        )}
      </div>
      {!busy && (
        <Button
          variant="primary"
          className="h-[28px] flex-none px-2.5 text-xs"
          onClick={install}
          disabled={recording}
          title={recording ? t("shell.updateWhileRecording") : undefined}
          aria-describedby={recording ? "update-line-detail" : undefined}
        >
          {t(u.phase.state === "error" ? "shell.updateRetry" : "shell.updateNow")}
        </Button>
      )}
    </div>
  );
}

const RecDot = () => <span aria-hidden="true" className="size-[9px] rounded-full bg-red shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-red)_30%,transparent)]" />;

/** The calendar meeting in progress or starting within the hour, with its own Start recording. */
function NextMeetingCard({ next, now, onStart, onOther }: { next: NextMeeting; now: number; onStart: () => void; onOther: () => void }) {
  const t = useT();
  const when = startsIn(next, now);
  const people = next.attendees.slice(0, 3).join(", ") + (next.attendees.length > 3 ? ", …" : "");
  return (
    <section aria-label={t("recording.nextAria", { title: next.title })} className="relative flex flex-col gap-3">
      <div className="flex flex-col gap-1 rounded-xl border border-line bg-paper px-3.5 py-3">
        <div className="flex items-center gap-2">
          <Label className="inline-flex flex-1 items-center gap-1.5">
            {when === "now" && <span aria-hidden="true" className="size-1.5 rounded-full bg-gold" />}
            {t("recording.nextMeeting")} · {when === "now" ? t("recording.nextNow") : t("recording.nextIn", { min: when })}
          </Label>
          {next.appLabel && <Chip>{next.appLabel}</Chip>}
        </div>
        <span className="truncate font-semibold">{next.title}</span>
        <span className="truncate text-xs leading-[17px] text-subtle">
          {formatClock(new Date(next.startMs).toISOString())}
          {people && ` · ${t("recording.nextWith", { names: people })}`}
        </span>
      </div>
      <Button variant="primary" className="h-[42px] w-full text-sm" onClick={onStart}>
        <RecDot />
        {t("recording.startRecording")}
      </Button>
      <button type="button" onClick={onOther} className="-mt-1 cursor-pointer self-center border-0 bg-transparent p-0 font-[inherit] text-xs text-subtle underline hover:text-ink">
        {t("recording.recordOther")}
      </button>
    </section>
  );
}
