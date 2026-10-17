import { describe, expect, it } from "vitest";
import { defaultSettings, shownCallApps, shownIgnoredApps, withDefaults } from "./defaultSettings";

describe("Mac-only apps", () => {
  it("are listed on the Mac and hidden on Windows", () => {
    const apps = defaultSettings.callApps;
    expect(shownCallApps(apps, false)).toEqual(apps);
    expect(shownCallApps(apps, true).map((a) => a.id)).not.toContain("facetime");
    expect(shownCallApps(apps, true)).toHaveLength(apps.length - 1);

    const ignored = [...defaultSettings.ignoredApps, { name: "Voice Memos", why: "built in" }, { name: "Krisp", why: "mic filter" }];
    expect(shownIgnoredApps(ignored, false)).toEqual(ignored);
    const names = shownIgnoredApps(ignored, true).map((a) => a.name);
    for (const mac of ["MacWhisper", "Apple Dictation", "Voice Memos"]) expect(names).not.toContain(mac);
    expect(names).toContain("Parlato");
    expect(names).toContain("Krisp");
  });
});

describe("withDefaults", () => {
  it("drops the old plain-text calendar address", () => {
    const s = withDefaults({ theme: "dark", calendarConnected: true, calendarIcsUrl: "https://example.com/secret.ics" } as never);
    expect("calendarIcsUrl" in s).toBe(false);
    expect(s.calendarConnected).toBe(true);
    expect(s.theme).toBe("dark");
  });
});

describe("call app rules", () => {
  it("ask before recording every app on a new install (FaceTime is never)", () => {
    for (const a of defaultSettings.callApps) expect(a.rule, a.id).toBe(a.id === "facetime" ? "never" : "ask");
  });

  it("keep a stored rule (no migration flips Always back to Ask)", () => {
    const s = withDefaults({ callApps: [{ id: "zoom", name: "Zoom", note: "zoom.us app", rule: "always" }] });
    expect(s.callApps.find((a) => a.id === "zoom")?.rule).toBe("always");
  });
});
