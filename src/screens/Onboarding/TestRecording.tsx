import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Mic } from "lucide-react";
import { recorderClient } from "../../data";
import { describeError } from "../../data/errors";
import { useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import { useRecorder } from "../../data/useRecorder";
import type { RecorderState } from "../../data/recorder";
import type { Settings } from "../../data/settings";
import type { Meeting } from "../../data/types";
import { formatTime } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { usePlatform } from "../../lib/platform";
import { navigate, routes } from "../../lib/router";
import { Button, useToast } from "../../ui";

/** The guided test stops by itself after a minute (the recorder does it, so it holds whatever screen is open). */
export const TEST_LIMIT_MS = 60_000;

/** Below this level for the first seconds, the test says it can't hear the person. */
const QUIET = 0.02;

/** True between pressing Start and the recorder answering with the new meeting (it appears in the list at once,
 *  so the empty screen would otherwise hide the card for a moment). */
let starting = false;
const startingListeners = new Set<() => void>();
const setStarting = (on: boolean) => {
  starting = on;
  startingListeners.forEach((l) => l());
};
const useStarting = () =>
  useSyncExternalStore(
    (l) => {
      startingListeners.add(l);
      return () => startingListeners.delete(l);
    },
    () => starting,
  );

/** The test recording is the one running now. Its meeting id is in settings from the moment it starts. */
const isTestRunning = (rec: RecorderState, s: Settings | undefined) => rec.state !== "idle" && Boolean(rec.meetingId) && rec.meetingId === s?.testMeetingId;

/** For the empty main screen: keep the card while the test is starting or running (its meeting is in the list). */
export function useTestRecordingActive(): boolean {
  const rec = useRecorder(recorderClient);
  const { settings } = useSettings();
  return useStarting() || isTestRunning(rec.state, settings);
}

/**
 * "Try a 1-minute test recording" on the empty main screen: the real recorder and pipeline, microphone only. The
 * recorder titles and tags the meeting as a test when it starts and stops it after a minute; the card only shows
 * the state, so it can come and go (TestRecordingWatcher opens the meeting when it ends).
 */
export function TestRecordingCard() {
  const t = useT();
  const platform = usePlatform();
  const toast = useToast();
  const { settings, update } = useSettings();
  const rec = useRecorder(recorderClient);
  const isStarting = useStarting();
  const [micOff, setMicOff] = useState(false);
  const [heard, setHeard] = useState(false);
  const stopRef = useRef<HTMLButtonElement>(null);
  const startRef = useRef<HTMLButtonElement>(null);
  const pressed = useRef(false);
  const running = isTestRunning(rec.state, settings);

  useEffect(() => {
    if (running && rec.levels.me > QUIET) setHeard(true);
  }, [running, rec.levels.me]);

  // Focus moves to the control that replaced the one pressed (Start → Stop, Cancel → Start).
  useEffect(() => {
    if (!pressed.current) return;
    pressed.current = false;
    (running ? stopRef : startRef).current?.focus();
  }, [running]);

  const start = async () => {
    setMicOff(false);
    setHeard(false);
    const perm = await recorderClient.microphonePermission().catch(() => "unknown" as const);
    if (perm === "denied" || perm === "restricted" || (perm === "undetermined" && !(await recorderClient.requestMicrophone().catch(() => false)))) {
      return setMicOff(true);
    }
    pressed.current = true;
    setStarting(true);
    try {
      const st = await recorderClient.start(undefined, { maxMs: TEST_LIMIT_MS });
      await update({ testMeetingId: st.meetingId || undefined });
    } catch (e) {
      toast(t("onboarding.testFailed", { error: describeError(e) }));
    } finally {
      setStarting(false);
    }
  };

  if (!settings) return null;
  const speech = settings.speechModels.find((m) => m.id === settings.activeSpeechModel);
  const modelNote =
    speech?.state === "downloading"
      ? t("onboarding.testModelDownloading", { pct: Math.floor((speech.progress ?? 0) * 100) })
      : !settings.speechModels.some((m) => m.state === "downloaded" || m.state === "on_this_mac")
        ? t("onboarding.testNoModel")
        : undefined;

  if (running) {
    const left = Math.max(0, TEST_LIMIT_MS - rec.state.elapsedMs);
    const level = Math.min(1, rec.levels.me * 1.6);
    return (
      <section aria-labelledby="test-title" className="flex w-full max-w-[420px] flex-col items-center gap-3 rounded-card border border-gold bg-surface px-6 py-5 shadow-[0_0_0_3px_var(--t-gold-soft)]">
        <span id="test-title" className="flex items-center gap-2 font-display text-lg font-bold tracking-[-0.02em]">
          <span aria-hidden="true" className={rec.state.state === "recording" ? "size-[9px] animate-pulse rounded-full bg-red" : "size-[9px] rounded-full bg-faint"} />
          {t("onboarding.testRecording")}
        </span>
        {/* Not live: the start and the end are announced once (TestRecordingWatcher), not every second. */}
        <span className="font-mono text-[28px] leading-none">{formatTime(left)}</span>
        <span aria-hidden="true" className="flex w-full items-center gap-2.5">
          <Mic size={15} className="flex-none text-subtle" />
          <span className="h-2 flex-1 overflow-hidden rounded-full bg-line">
            <span className="block h-full rounded-full bg-gold transition-[width] duration-150" style={{ width: `${Math.round(level * 100)}%` }} />
          </span>
        </span>
        <p className="m-0 text-[13px] text-subtle">{t("onboarding.testSay")}</p>
        {!heard && rec.state.elapsedMs >= 6_000 && (
          <p role="alert" className="m-0 text-[13px] text-warn-fg">
            {t("onboarding.testNoSound")}
          </p>
        )}
        <div className="flex gap-2">
          <Button
            variant="ghost"
            onClick={async () => {
              pressed.current = true;
              await recorderClient.discard().catch(() => {});
              await update({ testMeetingId: undefined });
            }}
          >
            {t("onboarding.testDiscard")}
          </Button>
          <Button ref={stopRef} variant="primary" raised onClick={() => void recorderClient.stop().catch((e) => toast(describeError(e)))}>
            <span aria-hidden="true" className="size-[9px] rounded-[2px] bg-red" />
            {t("onboarding.testStop")}
          </Button>
        </div>
      </section>
    );
  }

  return (
    <section aria-labelledby="test-title" className="flex w-full max-w-[420px] flex-col items-center gap-2.5 rounded-card border border-line bg-surface px-6 py-5 shadow-card">
      <span id="test-title" className="font-display text-lg font-bold tracking-[-0.02em]">
        {t("onboarding.testTitle")}
      </span>
      <p className="m-0 text-[13px] leading-[19px] text-subtle">{t("onboarding.testBody")}</p>
      {modelNote && <p className="m-0 text-xs leading-[17px] text-subtle">{modelNote}</p>}
      {micOff && (
        <div role="alert" className="flex flex-col items-center gap-2 rounded-card bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
          {t("onboarding.testMicOff")}
          <Button onClick={() => void recorderClient.openSettings("microphone")}>{platform.openSettings}</Button>
        </div>
      )}
      <Button ref={startRef} variant="primary" raised disabled={rec.state.state !== "idle" || isStarting} onClick={() => void start()}>
        <Mic size={15} aria-hidden="true" />
        {t("onboarding.testStart")}
      </Button>
    </section>
  );
}

/**
 * In the main window, whatever is open: when the test recording ends (Stop, the one-minute limit, the menu bar or
 * tray, a quit), open its meeting to watch the transcript appear. Says the start and the end to screen readers.
 */
export function TestRecordingWatcher() {
  const t = useT();
  const store = useStore();
  const { settings, update } = useSettings();
  const rec = useRecorder(recorderClient);
  const [said, setSaid] = useState("");
  const was = useRef<string | undefined>(undefined);
  const running = isTestRunning(rec.state, settings);

  useEffect(() => {
    if (running) {
      if (!was.current) setSaid(t("onboarding.testAnnStarted"));
      was.current = rec.state.meetingId;
      return;
    }
    const id = was.current;
    if (!id || rec.state.state !== "idle") return;
    was.current = undefined;
    void store.getMeeting(id).then((m) => {
      if (!m) return void update({ testMeetingId: undefined }); // cancelled: nothing to offer
      setSaid(t("onboarding.testAnnStopped"));
      navigate(routes.meeting(id));
    });
  }, [running, rec.state.state]);

  return (
    <span className="sr-only" aria-live="polite" role="status">
      {said}
    </span>
  );
}

/** On the test meeting's page: it was a test, delete it? Shown until the person deletes or keeps it. */
export function TestMeetingBanner({ meeting }: { meeting: Meeting }) {
  const t = useT();
  const toast = useToast();
  const store = useStore();
  const { settings, update } = useSettings();
  if (!settings || settings.testMeetingId !== meeting.id || meeting.status === "recording") return null;
  const busy = meeting.status === "transcribing";
  return (
    <div role="status" className="relative mx-11 mb-4 flex flex-wrap items-center gap-3 rounded-card border border-gold bg-gold-soft px-4 py-3 text-[13px] text-gold-ink">
      <span className="flex-1 basis-72">
        <strong className="font-semibold">{t("onboarding.testBannerTitle")}.</strong> {t("onboarding.testBannerBody")}
      </span>
      <Button variant="ghost" onClick={() => void update({ testMeetingId: undefined })}>
        {t("onboarding.testKeep")}
      </Button>
      <Button
        disabled={busy}
        onClick={async () => {
          try {
            await store.deleteMeeting(meeting.id);
            await update({ testMeetingId: undefined });
            toast(t("onboarding.testDeleted"));
            navigate(routes.meetings());
          } catch (e) {
            toast(describeError(e));
          }
        }}
      >
        {t("onboarding.testDelete")}
      </Button>
    </div>
  );
}
