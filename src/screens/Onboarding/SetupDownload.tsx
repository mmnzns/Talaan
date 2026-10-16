import { useEffect, useState } from "react";
import { useSettings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { Settings } from "../../data/settings";
import { useT } from "../../lib/i18n/I18nProvider";
import {
  batchView,
  gb,
  joinBatch,
  knownSpeed,
  minutesFor,
  missingMb,
  neededItems,
  probeSpeed,
  recommendedItems,
  type DownloadItem,
} from "../../lib/setupDownload";
import { cn } from "../../ui";

/**
 * The recommended setup: the speech model and the local summary model, downloaded together. The downloads run in
 * the background (Rust), so they keep going after onboarding and on every screen; this hook adds them up.
 */
export function useSetupDownload(s: Settings | undefined, { probe = false }: { probe?: boolean } = {}) {
  const store = useSettingsStore();
  const [speed, setSpeed] = useState(knownSpeed);
  const { speech, summary } = s ? recommendedItems(s) : {};
  const items = [speech, summary].filter((m): m is DownloadItem => Boolean(m));
  const missing = missingMb(items);
  const view = s ? batchView([...s.speechModels, ...s.localModels], neededItems(s)) : { active: false, fraction: 0, items: [] };

  // On the setup step only: measure the line once (a few MB from the model host, nothing saved), so the button can
  // say how long it takes before anything starts.
  useEffect(() => {
    if (probe && missing > 0) void probeSpeed(() => store.downloadSpeed()).then((v) => v && setSpeed(v));
  }, [probe, missing > 0, store]);

  const start = async () => {
    if (!s || !speech) return;
    // The recommended choices: this speech model, summaries written on this computer by the recommended model.
    await store.update({ activeSpeechModel: speech.id, summaryEngine: "local", localModel: "recommended" });
    const todo = items.filter((i) => i.state === "not_downloaded");
    joinBatch(todo.map((i) => i.id));
    await Promise.all(todo.map((i) => store.downloadModel(i.id)));
  };

  const cancel = () => Promise.all(view.items.filter((i) => i.state === "downloading").map((i) => store.cancelModelDownload(i.id)));

  return {
    speech,
    summary,
    missingMb: missing,
    /** Minutes for the whole recommended download at the measured speed (before it starts). */
    minutes: minutesFor(missing * 1_000_000, speed),
    view,
    start,
    cancel,
  };
}

/** One bar for both downloads, with the percentage and the time left. */
export function SetupProgress({ fraction, minutesLeft, className }: { fraction: number; minutesLeft?: number; className?: string }) {
  const t = useT();
  const pct = Math.floor(fraction * 100);
  return (
    <div className={cn("flex flex-col gap-1.5", className)} role="status">
      <span
        aria-hidden="true"
        className="h-2 overflow-hidden rounded-full bg-line"
      >
        <span className="block h-full rounded-full bg-gold transition-[width] duration-500" style={{ width: `${pct}%` }} />
      </span>
      <span className="flex flex-wrap justify-between gap-2 text-[13px]">
        <span className="font-semibold">{t("onboarding.setupDownloading", { pct })}</span>
        <span className="text-subtle">{minutesLeft ? t("onboarding.minLeft", { count: minutesLeft }) : t("onboarding.measuring")}</span>
      </span>
    </div>
  );
}

/** After onboarding: the recommended download's progress, small, in a corner of the main window. */
export function SetupDownloadPill() {
  const t = useT();
  const { settings } = useSettings();
  const { view } = useSetupDownload(settings);
  if (!view.active) return null;
  const pct = Math.floor(view.fraction * 100);
  return (
    <div
      role="status"
      title={t("onboarding.dlPillTitle")}
      className="fixed bottom-4 left-4 z-30 flex w-[280px] flex-col gap-1.5 rounded-card border border-line bg-raised px-3.5 py-2.5 shadow-[var(--t-shadow-lg)]"
    >
      <span className="text-xs font-semibold">{t("onboarding.dlPill", { pct })}</span>
      <span aria-hidden="true" className="h-1.5 overflow-hidden rounded-full bg-line">
        <span className="block h-full rounded-full bg-gold transition-[width] duration-500" style={{ width: `${pct}%` }} />
      </span>
      <span className="text-xs text-subtle">
        {view.minutesLeft ? t("onboarding.minLeft", { count: view.minutesLeft }) : t("onboarding.measuring")} · {t("onboarding.setupKeepUsing")}
      </span>
    </div>
  );
}

export { gb };
