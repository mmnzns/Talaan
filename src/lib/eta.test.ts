import { describe, expect, it } from "vitest";
import { etaText, savedText, timeLeft } from "./eta";
import { translate } from "./i18n";
import type { T } from "./i18n";

const now = Date.parse("2026-10-16T10:00:00Z");
const at = (ms: number) => ({ finishAt: new Date(now + ms).toISOString(), onCpu: false });
const en: T = (key, vars) => translate("en", key, vars);
const fr: T = (key, vars) => translate("fr", key, vars);

describe("timeLeft", () => {
  it("rounds to whole minutes from a minute up", () => {
    expect(timeLeft(at(8 * 60_000 + 10_000), now)).toEqual({ kind: "min", count: 8 });
    expect(timeLeft(at(90_000), now)).toEqual({ kind: "min", count: 2 });
    expect(timeLeft(at(60_000), now)).toEqual({ kind: "min", count: 1 });
  });
  it("says under a minute, then almost done once the estimate has passed by a minute", () => {
    expect(timeLeft(at(59_000), now)).toEqual({ kind: "soon" });
    expect(timeLeft(at(-30_000), now)).toEqual({ kind: "soon" });
    expect(timeLeft(at(-60_000), now)).toEqual({ kind: "over" });
  });
  it("ignores a broken time", () => {
    expect(timeLeft({ finishAt: "soon", onCpu: false }, now)).toBeUndefined();
  });
});

describe("etaText and savedText", () => {
  it("reads naturally in each language", () => {
    expect(etaText(en, at(8 * 60_000), now)).toBe("About 8 min left");
    expect(etaText(en, at(20_000), now)).toBe("Less than a minute left");
    expect(etaText(en, at(-5 * 60_000), now)).toBe("Almost done");
    expect(etaText(fr, at(60_000), now)).toBe("Environ 1 min restante");
    expect(etaText(en, undefined, now)).toBeUndefined();
    // Every chunk is done but speakers are still being told apart.
    expect(etaText(en, at(-10_000), now, 1)).toBe("Finishing up");
    expect(etaText(en, at(5 * 60_000), now, 0.5)).toBe("About 5 min left");
  });
  it("tells how long the transcript will take after Stop", () => {
    expect(savedText(en, at(4 * 60_000), now)).toBe("Recording saved. Writing the transcript now (about 4 min).");
    expect(savedText(en, at(10_000), now)).toBe("Recording saved. Writing the transcript now (less than a minute).");
    expect(savedText(en, undefined, now)).toBe("Recording saved. Writing the transcript now.");
  });
});
