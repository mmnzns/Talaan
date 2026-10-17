import { describe, expect, it } from "vitest";
import { throttled } from "./throttle";

describe("throttled", () => {
  it("skips while a run is going and until the gap has passed", async () => {
    let clock = 0;
    let runs = 0;
    let release!: () => void;
    const run = throttled(() => ((runs += 1), new Promise<void>((r) => (release = r))), 30_000, () => clock);
    const first = run();
    expect(await run(true)).toBe(false); // still running, even when forced
    release();
    expect(await first).toBe(true);
    clock = 10_000;
    expect(await run()).toBe(false); // too soon
    expect(runs).toBe(1);
    clock = 30_000;
    const again = run();
    release();
    expect(await again).toBe(true);
    expect(runs).toBe(2);
  });

  it("runs again after a failed job", async () => {
    let clock = 0;
    const run = throttled(() => Promise.reject(new Error("x")), 1000, () => clock);
    await expect(run()).rejects.toThrow("x");
    clock = 1000;
    await expect(run()).rejects.toThrow("x");
  });

  it("a forced run ignores the gap", async () => {
    let runs = 0;
    const run = throttled(async () => void (runs += 1), 30_000, () => 0);
    await run();
    expect(await run(true)).toBe(true);
    expect(runs).toBe(2);
  });
});
