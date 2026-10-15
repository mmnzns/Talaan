import { describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import type { Settings } from "../data/settings";
import { checkSetup, type Probe } from "./setupCheck";

const settings = (patch: Partial<Settings> = {}): Settings => ({ ...structuredClone(defaultSettings), ...patch });
const probe = (patch: Partial<Probe> = {}): Probe => ({
  settings: settings(),
  mic: "granted",
  inputs: ["Built-in microphone"],
  data: { path: "/data", bytes: 1_000_000 },
  ...patch,
});
const find = (p: Probe, id: string) => checkSetup(p).find((c) => c.id === id)!;

describe("checkSetup", () => {
  it("is all clear on a set-up computer, except call audio, which needs a test", () => {
    const results = checkSetup(probe());
    expect(results.map((r) => r.id)).toEqual(["mic", "device", "systemAudio", "speech", "speakers", "summary", "data"]);
    expect(results.filter((r) => r.status !== "ok").map((r) => r.id)).toEqual(["systemAudio", "speakers"]);
    expect(find(probe(), "systemAudio")).toMatchObject({ status: "unknown", fix: { action: "testSystemAudio" } });
    expect(find(probe({ systemAudio: true }), "systemAudio").status).toBe("ok");
    expect(find(probe({ systemAudio: false }), "systemAudio")).toMatchObject({ status: "fail", fix: { action: "systemAudioSettings" } });
  });

  it("says how to fix the microphone", () => {
    expect(find(probe({ mic: "undetermined" }), "mic")).toMatchObject({ status: "warn", fix: { action: "allowMic" } });
    expect(find(probe({ mic: "denied" }), "mic")).toMatchObject({ status: "fail", fix: { action: "micSettings" } });
    expect(find(probe({ inputs: [] }), "device").status).toBe("fail");
    const gone = find(probe({ settings: settings({ microphone: "Headset" }) }), "device");
    expect(gone.status).toBe("warn");
    expect(gone.detail).toContain("Headset");
  });

  it("checks the speech model the settings pick", () => {
    const models = defaultSettings.speechModels.map((m) => ({ ...m, state: "not_downloaded" as const }));
    expect(find(probe({ settings: settings({ speechModels: models }) }), "speech")).toMatchObject({ status: "fail", fix: { action: "speechModels" } });
    const other = models.map((m) => (m.id === "parakeet-v3" ? { ...m, state: "downloaded" as const } : m));
    const r = find(probe({ settings: settings({ speechModels: other }) }), "speech");
    expect(r.status).toBe("warn");
    expect(r.detail).toContain("Parakeet");
  });

  it("checks the summary engine that is chosen", () => {
    const noLocal = defaultSettings.localModels.map((m) => ({ ...m, state: "not_downloaded" as const }));
    expect(find(probe({ settings: settings({ localModels: noLocal }) }), "summary")).toMatchObject({ status: "fail", fix: { action: "summaryAI" } });
    expect(find(probe({ settings: settings({ summaryEngine: "byok", byokKeySet: false }) }), "summary").status).toBe("fail");
    expect(find(probe({ settings: settings({ summaryEngine: "byok", byokKeySet: true }) }), "summary").status).toBe("ok");
    const ollama = settings({ summaryEngine: "ollama", ollamaModel: "qwen3:8b" });
    expect(find(probe({ settings: ollama, localServer: { ok: false, message: "Couldn't connect" } }), "summary").status).toBe("fail");
    expect(find(probe({ settings: ollama, localServer: { ok: true, models: ["llama3"] } }), "summary").status).toBe("warn");
    expect(find(probe({ settings: ollama, localServer: { ok: true, models: ["qwen3:8b"] } }), "summary").status).toBe("ok");
    expect(find(probe({ settings: settings({ summarizeAutomatically: false }) }), "summary").detail).toContain("Generate summary");
  });

  it("on Windows, offers no Mac-only fix for call audio and checks the calendar's saved address", () => {
    expect(find(probe({ systemAudio: false, systemAudioNeedsPermission: false }), "systemAudio")).toMatchObject({ status: "fail", fix: undefined });
    expect(find(probe({ mic: "denied" }), "mic").fix?.action).toBe("micSettings");
    expect(checkSetup(probe()).some((r) => r.id === "calendar")).toBe(false);
    expect(find(probe({ calendar: "allowed" }), "calendar").status).toBe("ok");
    expect(find(probe({ calendar: "not_asked" }), "calendar")).toMatchObject({ status: "warn", fix: { action: "connections" } });
    expect(checkSetup(probe({ calendar: "allowed" })).map((r) => r.id).slice(-2)).toEqual(["calendar", "data"]);
  });

  it("reports a data folder it can't read", () => {
    expect(find(probe({ data: { error: "permission denied" } }), "data")).toMatchObject({ status: "fail" });
  });
});
