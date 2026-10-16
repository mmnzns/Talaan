import { useEffect, useRef, useState } from "react";
import { Mic } from "lucide-react";
import { recorderClient, simulateRecording } from "../../data";
import { describeError } from "../../data/errors";
import { useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import { useRecorder } from "../../data/useRecorder";
import type { Meeting } from "../../data/types";
import { formatTime } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { usePlatform } from "../../lib/platform";
import { navigate, routes } from "../../lib/router";
import { Button, useToast } from "../../ui";

/** The guided test stops by itself after a minute. */
export const TEST_LIMIT_MS = 60_000;
/** The tag that marks the test meeting (stored in meeting.json like any tag). */
export const TEST_TAG = "test";

/** Below this level for the first seconds, the test says it can't hear the person. */
const QUIET = 0.02;

/**
 * "Try a 1-minute test recording" on the empty main screen: the real recorder and the real pipeline (in the browser
 * preview, a simulated meeting). The meeting is titled and tagged as a test, and its page offers to delete it.
 */
export function TestRecordingCard() {
  const t = useT();
  const platform = usePlatform();
  const toast = useToast();
  const store = useStore();
  const { settings, update } = useSettings();
  const rec = useRecorder(recorderClient);
  const [mine, setMine] = useState(false); // this card started the recording that is running
  const [micOff, setMicOff] = useState(false);
  const [heard, setHeard] = useState(false);
  const meetingId = useRef<string | undefined>(undefined);
  const stopping = useRef(false);

  const running = mine && rec.state.state !== "idle";
  if (running && rec.state.meetingId) meetingId.current = rec.state.meetingId;

  useEffect(() => {
    if (running && rec.levels.me > QUIET) setHeard(true);
  }, [running, rec.levels.me]);

  const finish = async (id: string | undefined, elapsedMs: number) => {
    setMine(false);
    // The browser preview's recorder saves nothing: stand in a simulated meeting.
    const saved = id || simulateRecording?.(t("onboarding.testMeetingTitle"), Math.max(elapsedMs, 5_000), [TEST_TAG]);
    if (!saved) return;
    if (id) {
      await store.renameMeeting(id, t("onboarding.testMeetingTitle")).catch(() => {});
      await store.setTags(id, [TEST_TAG]).catch(() => {});
    }
    await update({ testMeetingId: saved });
    navigate(routes.meeting(saved));
  };

  const stop = async () => {
    if (stopping.current) return;
    stopping.current = true;
    const elapsed = rec.state.elapsedMs;
    try {
      const id = await recorderClient.stop();
      await finish(id || meetingId.current, elapsed);
    } catch (e) {
      toast(describeError(e));
    } finally {
      stopping.current = false;
    }
  };

  // Stops by itself at one minute.
  useEffect(() => {
    if (running && rec.state.elapsedMs >= TEST_LIMIT_MS) void stop();
  }, [running, rec.state.elapsedMs]);

  // Stopped somewhere else (the menu bar or tray popover): the meeting is saved the same way.
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running && mine && !stopping.current) void finish(meetingId.current, rec.state.elapsedMs);
    wasRunning.current = running;
  }, [running]);

  const start = async () => {
    setMicOff(false);
    setHeard(false);
    meetingId.current = undefined;
    const perm = await recorderClient.microphonePermission().catch(() => "unknown" as const);
    if (perm === "denied" || perm === "restricted" || (perm === "undetermined" && !(await recorderClient.requestMicrophone().catch(() => false)))) {
      return setMicOff(true);
    }
    try {
      await recorderClient.start();
      setMine(true);
    } catch (e) {
      toast(t("onboarding.testFailed", { error: describeError(e) }));
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
      <section aria-live="polite" className="flex w-full max-w-[420px] flex-col items-center gap-3 rounded-card border border-gold bg-surface px-6 py-5 shadow-[0_0_0_3px_var(--t-gold-soft)]">
        <span className="flex items-center gap-2 font-display text-lg font-bold tracking-[-0.02em]">
          <span aria-hidden="true" className={rec.state.state === "recording" ? "size-[9px] animate-pulse rounded-full bg-red" : "size-[9px] rounded-full bg-faint"} />
          {t("onboarding.testRecording")}
        </span>
        <span className="font-mono text-[28px] leading-none">{formatTime(left)}</span>
        <span className="flex w-full items-center gap-2.5">
          <Mic size={15} aria-hidden="true" className="flex-none text-subtle" />
          <span role="meter" aria-label={t("onboarding.testLevel")} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(level * 100)} className="h-2 flex-1 overflow-hidden rounded-full bg-line">
            <span className="block h-full rounded-full bg-gold transition-[width] duration-150" style={{ width: `${Math.round(level * 100)}%` }} />
          </span>
        </span>
        <p className="m-0 text-[13px] text-subtle">{t("onboarding.testSay")}</p>
        {!heard && rec.state.elapsedMs >= 6_000 && <p className="m-0 text-[13px] text-warn-fg">{t("onboarding.testNoSound")}</p>}
        <div className="flex gap-2">
          <Button
            variant="ghost"
            onClick={async () => {
              setMine(false);
              await recorderClient.discard().catch(() => {});
            }}
          >
            {t("onboarding.testDiscard")}
          </Button>
          <Button variant="primary" raised onClick={() => void stop()}>
            <span aria-hidden="true" className="size-[9px] rounded-[2px] bg-red" />
            {t("onboarding.testStop")}
          </Button>
        </div>
      </section>
    );
  }

  return (
    <section className="flex w-full max-w-[420px] flex-col items-center gap-2.5 rounded-card border border-line bg-surface px-6 py-5 shadow-card">
      <span className="font-display text-lg font-bold tracking-[-0.02em]">{t("onboarding.testTitle")}</span>
      <p className="m-0 text-[13px] leading-[19px] text-subtle">{t("onboarding.testBody")}</p>
      {modelNote && <p className="m-0 text-xs leading-[17px] text-subtle">{modelNote}</p>}
      {micOff && (
        <div role="alert" className="flex flex-col items-center gap-2 rounded-card bg-warn-bg px-3 py-2.5 text-[13px] text-warn-fg">
          {t("onboarding.testMicOff")}
          <Button onClick={() => void recorderClient.openSettings("microphone")}>{platform.openSettings}</Button>
        </div>
      )}
      <Button variant="primary" raised disabled={rec.state.state !== "idle"} onClick={() => void start()}>
        <Mic size={15} aria-hidden="true" />
        {t("onboarding.testStart")}
      </Button>
    </section>
  );
}

/** On the test meeting's page: it was a test, delete it? Shown until the person deletes or keeps it. */
export function TestMeetingBanner({ meeting }: { meeting: Meeting }) {
  const t = useT();
  const toast = useToast();
  const store = useStore();
  const { settings, update } = useSettings();
  if (!settings || settings.testMeetingId !== meeting.id) return null;
  const busy = meeting.status === "recording" || meeting.status === "transcribing";
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
