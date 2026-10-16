import { beforeEach, describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import { SpeedMeter, batchView, combined, joinBatch, minutesFor, missingMb, neededItems, probeSpeed, recommendedItems, resetBatchForTests, type DownloadItem } from "./setupDownload";

const item = (id: string, sizeMb: number, state: string, progress?: number): DownloadItem => ({ id, name: id, sizeMb, state, progress });

beforeEach(resetBatchForTests);

describe("the recommended setup", () => {
  it("is the recommended speech model and the recommended local summary model", () => {
    const { speech, summary } = recommendedItems(defaultSettings);
    expect(speech?.id).toBe("whisper-turbo");
    expect(summary?.id).toBe("qwen3.5-4b");
  });

  it("counts only what isn't on this computer yet", () => {
    expect(missingMb([item("a", 575, "not_downloaded"), item("b", 2741, "not_downloaded")])).toBe(3316);
    expect(missingMb([item("a", 575, "on_this_mac"), item("b", 2741, "not_downloaded")])).toBe(2741);
    expect(missingMb([item("a", 575, "downloaded"), item("b", 2741, "downloaded")])).toBe(0);
  });

  it("needs the local summary model only when summaries are written on this computer", () => {
    expect(neededItems(defaultSettings).map((m) => m.id)).toEqual(["whisper-turbo", "qwen3.5-4b"]);
    expect(neededItems({ ...defaultSettings, summaryEngine: "byok" }).map((m) => m.id)).toEqual(["whisper-turbo"]);
  });
});

describe("combined progress", () => {
  it("weighs each model by its size and counts finished ones in full", () => {
    const c = combined([item("a", 1000, "downloaded"), item("b", 3000, "downloading", 0.5)]);
    expect(c.totalBytes).toBe(4e9);
    expect(c.fraction).toBeCloseTo(2500 / 4000);
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
  it("keeps a finished model in the bar while the other one downloads, then clears", async () => {
    await probeSpeed(async () => 10e6);
    joinBatch(["a", "b"]);
    const v = batchView([item("a", 1000, "downloaded"), item("b", 1000, "downloading", 0.5), item("c", 50, "not_downloaded")], [], 0);
    expect(v.active).toBe(true);
    expect(v.items.map((i) => i.id)).toEqual(["a", "b"]);
    expect(v.fraction).toBeCloseTo(0.75);
    expect(v.minutesLeft).toBe(1); // 500 MB at the probed 10 MB/s
    const done = batchView([item("a", 1000, "downloaded"), item("b", 1000, "downloaded")], [], 1000);
    expect(done.active).toBe(false);
    expect(batchView([item("a", 1000, "downloading", 0.1)], [], 2000).active).toBe(false); // the batch was cleared
  });

  it("picks up a needed model downloading elsewhere (Settings)", () => {
    const speech = item("s", 600, "downloading", 0.2);
    expect(batchView([speech], [speech]).active).toBe(true);
  });
});
