/**
 * "Download what Talaan needs": the speech model and (when summaries are written on this computer) the local
 * summary model, fetched together with one progress bar and a time estimate. The downloads themselves run in Rust
 * (models_download) and keep going whatever screen is open; this file only adds them up and measures the speed
 * from the running download (nothing is measured before the person clicks). Pure, except for the module-level
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
  /** What an interrupted download left on disk (0..1). */
  partial?: number;
  /** Why the last download failed. */
  error?: string;
}

export const isReady = (i: DownloadItem) => i.state === "downloaded" || i.state === "on_this_mac";

/** The local summary model the settings pick. */
const localModel = (s: Settings) => s.localModels.find((m) => m.choice === (s.localModel ?? "recommended")) ?? s.localModels[0];

/**
 * What the setup step offers, and what its "all ready" and "Skip for now" look at: the active speech model when
 * it's already on this computer (else the recommended one), plus the local summary model only when summaries are
 * written on this computer. Someone who chose online AI is never switched back to local.
 */
export function setupItems(s: Settings): { speech?: DownloadItem; summary?: DownloadItem } {
  const active = s.speechModels.find((m) => m.id === s.activeSpeechModel);
  const speech = active && isReady(active) ? active : (s.speechModels.find((m) => m.id === RECOMMENDED_SPEECH) ?? active ?? s.speechModels[0]);
  return { speech, summary: s.summaryEngine === "local" ? localModel(s) : undefined };
}

/** The models Talaan needs with the current choices: the active speech model, and the local summary model when
 *  summaries are written on this computer. Used after onboarding (the progress card, resuming). */
export function neededItems(s: Settings): DownloadItem[] {
  const speech = s.speechModels.find((m) => m.id === s.activeSpeechModel);
  const local = s.summaryEngine === "local" ? localModel(s) : undefined;
  return [speech, local].filter((m): m is NonNullable<typeof m> => Boolean(m));
}

/** Megabytes still to download: what isn't on this computer yet, less what an interrupted download left. */
export function missingMb(items: DownloadItem[]): number {
  return items.filter((i) => !isReady(i)).reduce((n, i) => n + i.sizeMb * (1 - Math.min(1, Math.max(0, i.state === "downloading" ? (i.progress ?? 0) : (i.partial ?? 0)))), 0);
}

/** Bytes done and in total over `items`: a finished one counts in full, an interrupted one by what it left. */
export function combined(items: DownloadItem[]): { doneBytes: number; totalBytes: number; fraction: number } {
  let done = 0;
  let total = 0;
  for (const i of items) {
    const bytes = i.sizeMb * 1_000_000;
    total += bytes;
    if (isReady(i)) done += bytes;
    else done += bytes * Math.min(1, Math.max(0, i.state === "downloading" ? (i.progress ?? 0) : (i.partial ?? 0)));
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

/** The progress milestone a screen reader hears: 0, 25, 50 or 75 (100 is "done"). */
export const milestone = (fraction: number) => Math.min(75, Math.floor(Math.max(0, fraction) * 4) * 25);

/**
 * Download speed from progress samples: bytes over the last `windowMs`. Needs a few seconds of samples before it
 * answers ("estimating…" until then), so the first jumpy second doesn't promise "1 min".
 */
export class SpeedMeter {
  private samples: [number, number][] = [];
  constructor(
    private windowMs = 30_000,
    private minSpanMs = 3_000,
  ) {}

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

/** The models fetched together, kept while any of them is still downloading or failed (so one finishing early
 *  doesn't make the bar jump back, and a failure stays on screen with Retry). Module-level so it survives
 *  leaving onboarding. */
const batch = new Set<string>();
const meter = new SpeedMeter();

/**
 * Adds models to the shared batch (the recommended setup's button, Retry, resuming after a relaunch). The speed
 * clock starts now, at the request, with what is already on disk, not at the first progress report.
 */
export function joinBatch(ids: string[], all: DownloadItem[], now = Date.now()) {
  ids.forEach((id) => batch.add(id));
  meter.reset();
  meter.add(now, combined(all.filter((i) => batch.has(i.id))).doneBytes);
}

export interface BatchView {
  /** Something in the batch is downloading. */
  active: boolean;
  /** Models in the batch whose download failed (and isn't running again). */
  failed: DownloadItem[];
  fraction: number;
  /** Minutes left from the measured speed; undefined while estimating. */
  minutesLeft?: number;
  items: DownloadItem[];
}

/**
 * The batch as it stands in `all` (every model the settings know). Models of `needed` that are downloading join
 * the batch, so a download started in Settings shows too. Records a speed sample at `now`.
 */
export function batchView(all: DownloadItem[], needed: DownloadItem[], now = Date.now()): BatchView {
  for (const n of needed) if (n.state === "downloading" && !batch.has(n.id)) joinBatch([n.id], all, now);
  const items = all.filter((i) => batch.has(i.id));
  const active = items.some((i) => i.state === "downloading");
  const failed = items.filter((i) => i.state !== "downloading" && !isReady(i) && i.error);
  if (!active && failed.length === 0) {
    batch.clear();
    meter.reset();
    return { active: false, failed: [], fraction: 0, items: [] };
  }
  const { doneBytes, totalBytes, fraction } = combined(items);
  if (active) meter.add(now, doneBytes);
  return { active, failed, fraction, minutesLeft: active ? minutesFor(totalBytes - doneBytes, meter.speed()) : undefined, items };
}

/** Tests only. */
export function resetBatchForTests() {
  batch.clear();
  meter.reset();
}
