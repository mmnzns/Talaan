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

describe("the model lists", () => {
  it("list every speech model of the engine catalog, with licence and English-only", () => {
    expect(defaultSettings.speechModels).toHaveLength(22);
    expect(defaultSettings.speechModels.slice(0, 3).map((m) => m.id)).toEqual(["whisper-turbo", "parakeet-v3", "whisper-small"]);
    for (const m of defaultSettings.speechModels) expect(m.licence, m.id).toBeTruthy();
    const english = defaultSettings.speechModels.filter((m) => m.englishOnly).map((m) => m.id);
    expect(english).toContain("whisper-small-en");
    expect(english).toContain("parakeet-v2");
    expect(english).not.toContain("whisper-turbo");
  });

  it("offer three local AI models, Qwen3.5 4B recommended and in use", () => {
    expect(defaultSettings.localModels.map((m) => m.id)).toEqual(["qwen3.5-4b", "qwen3.5-9b", "granite-4.2-3b"]);
    expect(defaultSettings.localModels.filter((m) => m.recommended).map((m) => m.id)).toEqual(["qwen3.5-4b"]);
    expect(defaultSettings.localModel).toBe("qwen3.5-4b");
    expect("reuseModels" in defaultSettings).toBe(false);
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
