import { describe, expect, it, vi } from "vitest";
import { MockSettingsStore } from "./settings";

describe("MockSettingsStore", () => {
  it("updates, notifies and returns copies", async () => {
    const s = new MockSettingsStore();
    const fn = vi.fn();
    s.subscribe(fn);
    const next = await s.update({ retention: "30d" });
    expect(next.retention).toBe("30d");
    expect(fn).toHaveBeenCalled();
    next.retention = "none";
    expect((await s.get()).retention).toBe("30d");
  });

  it("downloads a model through to downloaded", async () => {
    vi.useFakeTimers();
    const s = new MockSettingsStore();
    await s.downloadModel("whisper-small");
    expect((await s.get()).speechModels.find((m) => m.id === "whisper-small")?.state).toBe("downloading");
    await vi.advanceTimersByTimeAsync(6000);
    expect((await s.get()).speechModels.find((m) => m.id === "whisper-small")?.state).toBe("downloaded");
    vi.useRealTimers();
  });

  it("tests API keys", async () => {
    vi.useFakeTimers();
    const s = new MockSettingsStore();
    const bad = s.testApiKey("claude", "short");
    await vi.advanceTimersByTimeAsync(800);
    expect((await bad).ok).toBe(false);
    const good = s.testApiKey("claude", "sk-test-1234567890");
    await vi.advanceTimersByTimeAsync(800);
    expect((await good).ok).toBe(true);
    vi.useRealTimers();
  });

  it("delete all data returns to first-run", async () => {
    vi.useFakeTimers();
    const s = new MockSettingsStore();
    const done = s.deleteAllData();
    await vi.advanceTimersByTimeAsync(500);
    await done;
    expect((await s.get()).onboardingDone).toBe(false);
    vi.useRealTimers();
  });
});
