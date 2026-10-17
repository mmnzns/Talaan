import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

import { MockRecorderClient, TauriRecorderClient } from "./recorder";

describe("TauriRecorderClient", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  it("calls the recorder commands with camelCase arguments", async () => {
    const c = new TauriRecorderClient();
    await c.start({ id: "zoom", label: "Zoom" });
    await c.start();
    await c.start(undefined, { maxMs: 60_000 });
    await c.mark("pricing");
    await c.mark();
    await c.openSettings("system-audio");
    expect(invoke.mock.calls).toEqual([
      ["recorder_start", { appId: "zoom", appLabel: "Zoom", test: null }],
      ["recorder_start", { appId: null, appLabel: null, test: null }],
      ["recorder_start", { appId: null, appLabel: null, test: { maxMs: 60_000 } }],
      ["recorder_mark", { note: "pricing" }],
      ["recorder_mark", { note: null }],
      ["permissions_open_settings", { pane: "system-audio" }],
    ]);
  });

  it("reads the microphone permission", async () => {
    invoke.mockResolvedValue({ microphone: "denied" });
    expect(await new TauriRecorderClient().microphonePermission()).toBe("denied");
  });
});

describe("MockRecorderClient", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("counts up while recording and freezes while paused", async () => {
    const c = new MockRecorderClient();
    const states: string[] = [];
    c.onState((s) => states.push(`${s.state}:${s.elapsedMs}`));
    await c.start();
    vi.advanceTimersByTime(3000);
    await c.pause();
    vi.advanceTimersByTime(5000);
    expect((await c.status()).elapsedMs).toBe(3000);
    await c.resume();
    vi.advanceTimersByTime(1000);
    expect((await c.status()).elapsedMs).toBe(4000);
    await c.mark();
    await c.mark("note");
    expect(await c.status()).toMatchObject({ marks: 2, notes: 1 });
    await c.stop();
    expect((await c.status()).state).toBe("idle");
    expect(states[0]).toBe("recording:0");
  });

  it("sends levels while recording only", async () => {
    const c = new MockRecorderClient();
    const seen: number[] = [];
    c.onLevels((l) => seen.push(l.them));
    await c.start();
    vi.advanceTimersByTime(600);
    expect(seen.length).toBeGreaterThan(2);
    expect(seen.every((v) => v > 0)).toBe(true);
    await c.stop();
  });
});

describe("MockRecorderClient with the preview's meetings", () => {
  afterEach(() => vi.useRealTimers());

  it("creates the meeting on Start and saves it on Stop, as the app does", async () => {
    vi.useFakeTimers();
    const calls: string[] = [];
    const c = new MockRecorderClient({ meetings: { start: (test) => (calls.push(`start ${test}`), "m-1"), stop: (id, ms) => calls.push(`stop ${id} ${ms}`), discard: (id) => calls.push(`discard ${id}`) } });
    expect((await c.start()).meetingId).toBe("m-1");
    await vi.advanceTimersByTimeAsync(3000);
    expect(await c.stop()).toBe("m-1");
    await c.start();
    await c.discard();
    expect(calls).toEqual(["start false", "stop m-1 3000", "start false", "discard m-1"]);
  });

  it("stops the test recording by itself at its limit", async () => {
    vi.useFakeTimers();
    const stops: number[] = [];
    const c = new MockRecorderClient({ meetings: { start: () => "t-1", stop: (_id, ms) => stops.push(ms), discard: () => {} } });
    await c.start(undefined, { maxMs: 5000 });
    await vi.advanceTimersByTimeAsync(6000);
    expect(stops).toEqual([5000]);
    expect((await c.status()).state).toBe("idle");
  });
});
