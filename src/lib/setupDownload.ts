/**
 * "Download what Talaan needs": the recommended speech model and the local summary model, fetched together with
 * one progress bar and a time estimate. The downloads themselves run in Rust (models_download) and keep going
 * whatever screen is open; this file only adds them up and measures the speed. Pure, except for the module-level
 * batch and speed meter, which outlive any one screen (onboarding hands over to the main window).
 */
import type { Settings } from "../data/settings";

/** The speech model onboarding recommends (the Rust catalog's default). */
export const RECOMMENDED_SPEECH = "whisper-turbo";

export interface DownloadItem {
  id: string;
  name: string;
  sizeMb: number;
  state: string; // "not_downloaded" | "downloading" | "downloaded" | "on_this_mac"
  progress?: number;
}

const ready = (i: DownloadItem) => i.state === "downloaded" || i.state === "on_this_mac";

/** What the recommended setup consists of: the recommended speech model and the recommended local summary model. */
export function recommendedItems(s: Settings): { speech?: DownloadItem; summary?: DownloadItem } {
  const speech = s.speechModels.find((m) => m.id === RECOMMENDED_SPEECH) ?? s.speechModels[0];
  const summary = s.localModels.find((m) => m.choice === "recommended") ?? s.localModels[0];
  return { speech, summary };
}

/** The models Talaan needs with the current choices: the active speech model, and the local summary model when
 *  summaries are written on this computer. Used to show the download after onboarding. */
export function neededItems(s: Settings): DownloadItem[] {
  const speech = s.speechModels.find((m) => m.id === s.activeSpeechModel);
  const local = s.summaryEngine === "local" ? s.localModels.find((m) => m.choice === (s.localModel ?? "recommended")) : undefined;
  return [speech, local].filter((m): m is NonNullable<typeof m> => Boolean(m));
}

/** Megabytes still to download (anything not on this computer yet). */
export function missingMb(items: DownloadItem[]): number {
  return items.filter((i) => !ready(i)).reduce((n, i) => n + i.sizeMb, 0);
}

/** Bytes done and in total over `items`, where a finished one counts in full. */
export function combined(items: DownloadItem[]): { doneBytes: number; totalBytes: number; fraction: number } {
  let done = 0;
  let total = 0;
  for (const i of items) {
    const bytes = i.sizeMb * 1_000_000;
    total += bytes;
    if (ready(i)) done += bytes;
    else if (i.state === "downloading") done += bytes * Math.min(1, Math.max(0, i.progress ?? 0));
  }
  return { doneBytes: done, totalBytes: total, fraction: total > 0 ? done / total : 0 };
}

/** Whole minutes for "about N min", rounded up, never 0. Undefined without a speed. */
export function minutesFor(bytes: number, bytesPerSecond: number | undefined): number | undefined {
  if (!bytesPerSecond || bytesPerSecond <= 0 || !Number.isFinite(bytesPerSecond)) return undefined;
  return Math.max(1, Math.ceil(bytes / bytesPerSecond / 60));
}

/** GB with one decimal, as the onboarding buttons show sizes. */
export const gb = (mb: number) => (mb / 1000).toFixed(1);

/**
 * Download speed from progress samples: bytes over the last `windowMs`. Needs a few seconds of samples before it
 * answers, so the first jumpy second doesn't promise "1 min".
 */
export class SpeedMeter {
  private samples: [number, number][] = [];
  constructor(private windowMs = 30_000, private minSpanMs = 3_000) {}

  add(atMs: number, bytes: number) {
    const last = this.samples[this.samples.length - 1];
    if (last && bytes < last[1]) this.samples = []; // a new batch or a cancel: start over
    if (last && last[1] === bytes) return;
    this.samples.push([atMs, bytes]);
    while (this.samples.length > 2 && atMs - this.samples[0][0] > this.windowMs) this.samples.shift();
  }

  reset() {
    this.samples = [];
  }

  /** Bytes per second, or undefined until there is enough to go on. */
  speed(): number | undefined {
    if (this.samples.length < 2) return undefined;
    const [t0, b0] = this.samples[0];
    const [t1, b1] = this.samples[this.samples.length - 1];
    if (t1 - t0 < this.minSpanMs || b1 <= b0) return undefined;
    return ((b1 - b0) / (t1 - t0)) * 1000;
  }
}

/** The models fetched together, kept while any of them is still downloading (so one finishing early doesn't make
 *  the bar jump back). Module-level so it survives leaving onboarding. */
const batch = new Set<string>();
const meter = new SpeedMeter();
let probed: number | undefined;
let probing: Promise<number | undefined> | undefined;

/** Adds models to the shared batch (the recommended setup's button). */
export function joinBatch(ids: string[]) {
  ids.forEach((id) => batch.add(id));
}

export interface BatchView {
  /** Something in the batch is downloading. */
  active: boolean;
  fraction: number;
  /** Minutes left, from the measured speed (or the probe until there are samples). */
  minutesLeft?: number;
  items: DownloadItem[];
}

/**
 * The batch as it stands in `all` (every model the settings know). Models of `needed` that are downloading join
 * the batch, so a download started in Settings shows too. Records a speed sample at `now`.
 */
export function batchView(all: DownloadItem[], needed: DownloadItem[], now = Date.now()): BatchView {
  for (const n of needed) if (n.state === "downloading") batch.add(n.id);
  const items = all.filter((i) => batch.has(i.id));
  const active = items.some((i) => i.state === "downloading");
  if (!active) {
    batch.clear();
    meter.reset();
    return { active: false, fraction: 0, items: [] };
  }
  const { doneBytes, totalBytes, fraction } = combined(items);
  meter.add(now, doneBytes);
  return { active, fraction, minutesLeft: minutesFor(totalBytes - doneBytes, meter.speed() ?? probed), items };
}

/** Measures the line once per launch (a few MB, nothing saved) for the estimate before anything starts. */
export function probeSpeed(measure: () => Promise<number | undefined>): Promise<number | undefined> {
  if (probed !== undefined) return Promise.resolve(probed);
  probing ??= measure()
    .then((s) => (probed = s && s > 0 ? s : undefined))
    .catch(() => undefined)
    .finally(() => (probing = undefined));
  return probing;
}

/** The speed measured so far: from the running download, else the probe. */
export function knownSpeed(): number | undefined {
  return meter.speed() ?? probed;
}

/** Tests only. */
export function resetBatchForTests() {
  batch.clear();
  meter.reset();
  probed = undefined;
  probing = undefined;
}
