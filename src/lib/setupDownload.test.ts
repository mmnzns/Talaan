import { beforeEach, describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import type { Settings } from "../data/settings";
import { SpeedMeter, batchView, combined, joinBatch, milestone, minutesFor, missingMb, neededItems, resetBatchForTests, setupItems, type DownloadItem } from "./setupDownload";

const item = (id: string, sizeMb: number, state: string, more: Partial<DownloadItem> = {}): DownloadItem => ({ id, name: id, sizeMb, state, ...more });
const none = <T extends { state: string }>(m: T): T => ({ ...m, state: "not_downloaded" });
const fresh: Settings = { ...defaultSettings, speechModels: defaultSettings.speechModels.map(none), localModels: defaultSettings.localModels.map(none) };

beforeEach(resetBatchForTests);

describe("the setup step's models", () => {
  it("are the recommended speech model and the recommended local summary model", () => {
    const { speech, summary } = setupItems(fresh);
    expect(speech?.id).toBe("whisper-turbo");
    expect(summary?.id).toBe("qwen3.5-4b");
  });

  it("never include the local summary model for someone who chose online AI", () => {
    expect(setupItems({ ...fresh, summaryEngine: "byok" }).summary).toBeUndefined();
  });

  it("keep a speech model that is already on this computer", () => {
    const s = { ...fresh, activeSpeechModel: "parakeet-v3", speechModels: fresh.speechModels.map((m) => (m.id === "parakeet-v3" ? { ...m, state: "on_this_mac" as const } : m)) };
    expect(setupItems(s).speech?.id).toBe("parakeet-v3");
  });

  it("count only what isn't on this computer yet, less what an interrupted download left", () => {
    expect(missingMb([item("a", 575, "not_downloaded"), item("b", 2741, "not_downloaded")])).toBe(3316);
    expect(missingMb([item("a", 575, "on_this_mac"), item("b", 2741, "not_downloaded")])).toBe(2741);
    expect(missingMb([item("a", 1000, "not_downloaded", { partial: 0.25 })])).toBe(750);
    expect(missingMb([item("a", 575, "downloaded"), item("b", 2741, "downloaded")])).toBe(0);
  });

  it("are what's needed after onboarding too", () => {
    expect(neededItems(defaultSettings).map((m) => m.id)).toEqual(["whisper-turbo", "qwen3.5-4b"]);
    expect(neededItems({ ...defaultSettings, summaryEngine: "byok" }).map((m) => m.id)).toEqual(["whisper-turbo"]);
  });
});

describe("combined progress", () => {
  it("weighs each model by its size and counts finished and partial ones", () => {
    const c = combined([item("a", 1000, "downloaded"), item("b", 3000, "downloading", { progress: 0.5 }), item("c", 1000, "not_downloaded", { partial: 0.5 })]);
    expect(c.totalBytes).toBe(5e9);
    expect(c.fraction).toBeCloseTo(3000 / 5000);
  });

  it("announces only every 25%", () => {
    expect([0, 0.1, 0.25, 0.49, 0.5, 0.8, 0.99, 1].map(milestone)).toEqual([0, 0, 25, 25, 50, 75, 75, 75]);
  });
});

describe("time left", () => {
  it("rounds up to whole minutes and never says 0", () => {
    expect(minutesFor(3.3e9, 10e6)).toBe(6); // 330 s
    expect(minutesFor(1e6, 10e6)).toBe(1);
    expect(minutesFor(1e9, undefined)).toBeUndefined();
    expect(minutesFor(1e9, 0)).toBeUndefined();
  });

  it("measures the speed only after a few seconds of samples", () => {
    const m = new SpeedMeter(30_000, 3_000);
    m.add(0, 0);
    m.add(1000, 10e6);
    expect(m.speed()).toBeUndefined();
    m.add(4000, 40e6);
    expect(m.speed()).toBeCloseTo(10e6);
  });

  it("starts over when the bytes go backwards", () => {
    const m = new SpeedMeter(30_000, 1000);
    m.add(0, 0);
    m.add(2000, 20e6);
    m.add(3000, 0);
    expect(m.speed()).toBeUndefined();
  });
});

describe("the batch", () => {
  it("starts the clock at the click: estimating at first, then minutes from the real download", () => {
    const all0 = [item("a", 1000, "not_downloaded"), item("b", 1000, "not_downloaded")];
    joinBatch(["a", "b"], all0, 0);
    expect(batchView([item("a", 1000, "downloading", { progress: 0 }), item("b", 1000, "downloading", { progress: 0 })], [], 0).minutesLeft).toBeUndefined();
    // 10 MB/s from the click: 100 MB after 10 s, 1.9 GB left = 190 s.
    const v = batchView([item("a", 1000, "downloading", { progress: 0.1 }), item("b", 1000, "downloading", { progress: 0 })], [], 10_000);
    expect(v.active).toBe(true);
    expect(v.minutesLeft).toBe(4);
  });

  it("keeps a finished model in the bar while the other one downloads, then clears", () => {
    joinBatch(["a", "b"], [], 0);
    const v = batchView([item("a", 1000, "downloaded"), item("b", 1000, "downloading", { progress: 0.5 }), item("c", 50, "not_downloaded")], [], 0);
    expect(v.items.map((i) => i.id)).toEqual(["a", "b"]);
    expect(v.fraction).toBeCloseTo(0.75);
    expect(batchView([item("a", 1000, "downloaded"), item("b", 1000, "downloaded")], [], 1000).active).toBe(false);
    expect(batchView([item("a", 1000, "downloading", { progress: 0.1 })], [], 2000).active).toBe(false); // cleared
  });

  it("stays visible with the failure until it's retried", () => {
    joinBatch(["a", "b"], [], 0);
    const v = batchView([item("a", 1000, "downloaded"), item("b", 1000, "not_downloaded", { partial: 0.4, error: "stalled" })], [], 0);
    expect(v.active).toBe(false);
    expect(v.failed.map((i) => i.id)).toEqual(["b"]);
    expect(v.fraction).toBeCloseTo(0.7);
  });

  it("picks up a needed model downloading elsewhere (Settings)", () => {
    const speech = item("s", 600, "downloading", { progress: 0.2 });
    expect(batchView([speech], [speech]).active).toBe(true);
  });
});
