import { useEffect, useRef, useState } from "react";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { useT } from "../../lib/i18n/I18nProvider";
import { batchView, cancelledPartials, gb, isReady, joinBatch, milestone, missingMb, neededItems, setupItems, toResume, type BatchView, type DownloadItem } from "../../lib/setupDownload";
import { Button, cn } from "../../ui";

const allModels = (s: Settings): DownloadItem[] => [...s.speechModels, ...s.localModels];

/**
 * The recommended setup: the speech model and (when summaries are written on this computer) the local summary
 * model, downloaded together. The downloads run in the background (Rust), so they keep going after onboarding and
 * on every screen; this hook adds them up. Nothing is contacted before `start`.
 */
export function useSetupDownload(s: Settings | undefined) {
  const store = useSettingsStore();
  const { speech, summary } = s ? setupItems(s) : {};
  const items = [speech, summary].filter((m): m is DownloadItem => Boolean(m));
  const view: BatchView = s ? batchView(allModels(s), neededItems(s)) : { active: false, failed: [], fraction: 0, items: [] };

  const fetch = async (todo: DownloadItem[]) => {
    if (!s) return;
    joinBatch(
      todo.map((i) => i.id),
      allModels(s),
    );
    await Promise.all(todo.map((i) => store.downloadModel(i.id)));
  };

  const start = async () => {
    if (!s || !speech) return;
    // The speech model becomes the active one; the summary engine is left as chosen (online AI stays online).
    if (s.activeSpeechModel !== speech.id) await store.update({ activeSpeechModel: speech.id });
    await fetch(items.filter((i) => i.state === "not_downloaded"));
  };

  const retry = () => fetch(view.failed);
  const cancel = () => Promise.all(view.items.filter((i) => i.state === "downloading").map((i) => store.cancelModelDownload(i.id)));

  return {
    speech,
    summary,
    /** Megabytes still to fetch (less what an interrupted download left). */
    missingMb: missingMb(items),
    /** Part of it is already on disk from an earlier try. */
    resumable: items.some((i) => !isReady(i) && (i.partial ?? 0) > 0),
    view,
    start,
    retry,
    cancel,
  };
}

/** What a screen reader hears about a download: started, every 25%, done, failed. Visually hidden; the bar and
 *  the percentage on screen are not live, so nothing is read out every second. */
export function DownloadAnnouncer({ view }: { view: BatchView }) {
  const t = useT();
  const [text, setText] = useState("");
  const last = useRef<string>("");
  const was = useRef(false);
  useEffect(() => {
    let next = "";
    if (view.failed.length > 0) next = t("onboarding.annFailed", { error: view.failed[0].error ?? "" });
    else if (view.active) {
      const m = milestone(view.fraction);
      next = m === 0 ? t("onboarding.annStarted") : t("onboarding.annProgress", { pct: m });
    } else if (was.current) next = t("onboarding.annDone");
    was.current = view.active;
    if (next && next !== last.current) {
      last.current = next;
      setText(next);
    }
  }, [view.active, view.failed.length, milestone(view.fraction)]);
  return (
    <span className="sr-only" aria-live="polite" role="status">
      {text}
    </span>
  );
}

/** One bar for both downloads, with the percentage and the time left (not live: see DownloadAnnouncer). */
export function SetupProgress({ fraction, minutesLeft, failed, className }: { fraction: number; minutesLeft?: number; failed?: boolean; className?: string }) {
  const t = useT();
  const pct = Math.floor(fraction * 100);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <span aria-hidden="true" className="h-2 overflow-hidden rounded-full bg-line">
        <span className={cn("block h-full rounded-full transition-[width] duration-500", failed ? "bg-faint" : "bg-gold")} style={{ width: `${pct}%` }} />
      </span>
      <span className="flex flex-wrap justify-between gap-2 text-[13px]">
        <span className="font-semibold">{failed ? t("onboarding.setupStopped", { pct }) : t("onboarding.setupDownloading", { pct })}</span>
        {!failed && <span className="text-subtle">{minutesLeft ? t("onboarding.minLeft", { count: minutesLeft }) : t("onboarding.measuring")}</span>}
      </span>
    </div>
  );
}

/** Resumes, once per launch, the needed models a quit or a crash left half-downloaded (quietly: the card shows).
 *  One the person cancelled waits for "Download the rest". */
let resumed = false;
/** "Not now" on "Download the rest": hidden until the next launch. */
let restDismissed = false;

/** After onboarding: the download's progress (or its failure, with Try again), small, in the corner of the main
 *  pane, clear of the meeting list. */
export function SetupDownloadPill() {
  const t = useT();
  const store = useSettingsStore();
  const { settings } = useSettings();
  const setup = useSetupDownload(settings);
  const { view } = setup;

  useEffect(() => {
    if (resumed || !settings?.onboardingDone) return;
    resumed = true;
    const todo = toResume(neededItems(settings));
    if (todo.length === 0) return;
    joinBatch(
      todo.map((m) => m.id),
      allModels(settings),
    );
    for (const m of todo) void store.downloadModel(m.id);
  }, [settings?.onboardingDone]);

  const [, hide] = useState(0);
  const card = "fixed bottom-4 left-[326px] z-30 flex w-[280px] flex-col gap-1.5 rounded-card border border-line bg-raised px-3.5 py-2.5 shadow-[var(--t-shadow-lg)]";
  if (!view.active && view.failed.length === 0) {
    // A download the person cancelled: offered, never resumed by itself.
    const rest = settings && !restDismissed ? cancelledPartials(neededItems(settings)) : [];
    if (rest.length === 0) return null;
    const pct = Math.floor(combinedFraction(rest) * 100);
    return (
      <div className={card}>
        <span className="text-xs font-semibold">{t("onboarding.dlRestTitle", { pct })}</span>
        <div className="flex flex-wrap gap-2">
          <Button
            onClick={() => {
              joinBatch(
                rest.map((m) => m.id),
                allModels(settings!),
              );
              for (const m of rest) void store.downloadModel(m.id);
            }}
          >
            {t("onboarding.dlRestButton", { size: gb(missingMb(rest)) })}
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              restDismissed = true;
              hide((n) => n + 1);
            }}
          >
            {t("onboarding.dlRestLater")}
          </Button>
        </div>
      </div>
    );
  }
  const failed = !view.active && view.failed.length > 0;
  const pct = Math.floor(view.fraction * 100);
  // Name only what is still coming: the speech model, the summary model, or both.
  const pending = view.items.filter((i) => !isReady(i));
  const speechIds = new Set(settings?.speechModels.map((m) => m.id));
  const hasSpeech = pending.some((i) => speechIds.has(i.id));
  const hasSummary = pending.some((i) => !speechIds.has(i.id));
  const title = hasSpeech && hasSummary ? t("onboarding.dlPillTitle") : hasSpeech ? t("onboarding.dlPillTitleSpeech") : t("onboarding.dlPillTitleSummary");
  return (
    <div
      title={title}
      className={card}
    >
      <DownloadAnnouncer view={view} />
      <span className="text-xs font-semibold">{failed ? t("onboarding.dlPillFailed", { pct }) : t("onboarding.dlPill", { pct })}</span>
      <span aria-hidden="true" className="h-1.5 overflow-hidden rounded-full bg-line">
        <span className={cn("block h-full rounded-full transition-[width] duration-500", failed ? "bg-faint" : "bg-gold")} style={{ width: `${pct}%` }} />
      </span>
      {failed ? (
        <>
          <span className="text-xs leading-[17px] text-warn-fg">{view.failed[0].error}</span>
          <Button className="self-start" onClick={() => void setup.retry()}>
            {t("onboarding.setupRetry")}
          </Button>
        </>
      ) : (
        <span className="text-xs text-subtle">
          {view.minutesLeft ? t("onboarding.minLeft", { count: view.minutesLeft }) : t("onboarding.measuring")} · {t("onboarding.setupKeepUsing")}
        </span>
      )}
    </div>
  );
}

const combinedFraction = (items: DownloadItem[]) => {
  const total = items.reduce((n, i) => n + i.sizeMb, 0);
  return total > 0 ? items.reduce((n, i) => n + i.sizeMb * (i.partial ?? 0), 0) / total : 0;
};

export { gb };
