import { describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import type { AutoSaveStatus, Settings } from "../data/settings";
import { setupLines } from "./diagnostics";
import { checkSetup, lightChecks, needsFixing, probeLight, type Probe } from "./setupCheck";

const settings = (patch: Partial<Settings> = {}): Settings => ({ ...structuredClone(defaultSettings), ...patch });
const probe = (patch: Partial<Probe> = {}): Probe => ({
  settings: settings(),
  mic: "granted",
  inputs: ["Built-in microphone"],
  data: { path: "/data", bytes: 1_000_000 },
  ...patch,
});
const find = (p: Probe, id: string) => checkSetup(p).find((c) => c.id === id)!;

describe("needsFixing (the dot on the Settings button)", () => {
  it("is empty on a set-up computer, even before call audio is tested", () => {
    expect(needsFixing(checkSetup(probe()))).toEqual([]);
  });
  it("never counts the summary check: transcript only is a choice, and a local server may be off", () => {
    const noLocal = defaultSettings.localModels.map((m) => ({ ...m, state: "not_downloaded" as const }));
    const results = checkSetup(probe({ settings: settings({ localModels: noLocal }) }));
    expect(results.find((r) => r.id === "summary")?.status).toBe("fail");
    expect(needsFixing(results)).toEqual([]);
    const ollama = settings({ summaryEngine: "ollama" });
    expect(needsFixing(checkSetup(probe({ settings: ollama, localServer: { ok: false, message: "Couldn't connect" } })))).toEqual([]);
    expect(needsFixing(checkSetup(probe({ settings: settings({ summaryEngine: "byok", byokKeys: [] }) })))).toEqual([]);
  });

  it("counts what stops a call, not notices", () => {
    expect(needsFixing(checkSetup(probe({ mic: "denied" }))).map((r) => r.id)).toEqual(["mic"]);
    const noSpeech = defaultSettings.speechModels.map((m) => ({ ...m, state: "not_downloaded" as const }));
    expect(needsFixing(checkSetup(probe({ settings: settings({ speechModels: noSpeech }) }))).map((r) => r.id)).toEqual(["speech"]);
    const notices = probe({ soundRelay: "FxSound", mic: "undetermined", gpu: { state: "failed", detail: "lost" } });
    expect(checkSetup(notices).filter((r) => r.status === "warn").length).toBeGreaterThan(0);
    expect(needsFixing(checkSetup(notices))).toEqual([]);
  });
});

describe("the light look (the Settings dot)", () => {
  it("asks only for settings, microphone access and the microphones", async () => {
    const asked: string[] = [];
    const store = {
      get: async () => (asked.push("get"), settings()),
      audioInputs: async () => (asked.push("audioInputs"), ["Built-in microphone"]),
    };
    const recorder = { microphonePermission: async () => (asked.push("mic"), "granted" as const) };
    const p = await probeLight(store, recorder);
    expect(asked.sort()).toEqual(["audioInputs", "get", "mic"]);
    expect(p).toMatchObject({ mic: "granted", inputs: ["Built-in microphone"] });
  });

  it("judges microphone access, the microphone and the speech model, the same way Status does", async () => {
    const light = { settings: settings(), mic: "denied" as const, inputs: [] };
    expect(lightChecks(light).map((r) => r.id)).toEqual(["mic", "device", "speech"]);
    expect(needsFixing(lightChecks(light)).map((r) => r.id)).toEqual(["mic", "device"]);
    const full = checkSetup(probe(light));
    for (const r of lightChecks(light)) expect(full.find((x) => x.id === r.id)).toEqual(r);
  });

  it("keeps going when the microphones can't be listed", async () => {
    const p = await probeLight({ get: async () => settings(), audioInputs: () => Promise.reject(new Error("x")) }, { microphonePermission: async () => "granted" as const });
    expect(p.inputs).toBeUndefined();
    expect(needsFixing(lightChecks(p))).toEqual([]);
  });
});

describe("checkSetup", () => {
  it("is all clear on a set-up computer, except call audio, which needs a test", () => {
    const results = checkSetup(probe());
    expect(results.map((r) => r.id)).toEqual(["mic", "device", "systemAudio", "speech", "speakers", "summary", "data"]);
    expect(results.filter((r) => r.status !== "ok").map((r) => r.id)).toEqual(["systemAudio", "speakers"]);
    expect(find(probe(), "systemAudio")).toMatchObject({ status: "unknown", fix: { action: "testSystemAudio" } });
    expect(find(probe({ systemAudio: true }), "systemAudio").status).toBe("ok");
    expect(find(probe({ systemAudio: false }), "systemAudio")).toMatchObject({ status: "fail", fix: { action: "systemAudioSettings" } });
  });

  it("on Windows, names the sound enhancer the sound goes through, after call audio", () => {
    expect(checkSetup(probe({ soundRelay: null })).some((r) => r.id === "relay")).toBe(false);
    const p = probe({ soundRelay: "FxSound" });
    expect(checkSetup(p).map((r) => r.id).slice(0, 4)).toEqual(["mic", "device", "systemAudio", "relay"]);
    const r = find(p, "relay");
    expect(r.status).toBe("warn");
    expect(r.detail).toBe("Your sound goes through FxSound, so Talaan may hear its own sounds. If the transcript repeats lines, record with FxSound off.");
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
    expect(find(probe({ settings: settings({ summaryEngine: "byok", byokProvider: "openai", byokKeys: ["claude"] }) }), "summary").status).toBe("fail");
    expect(find(probe({ settings: settings({ summaryEngine: "byok", byokProvider: "openai", byokKeys: ["claude", "openai"] }) }), "summary").status).toBe("ok");
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

  it("shows the graphics chip after speakers, and never as a failure", () => {
    expect(checkSetup(probe()).some((r) => r.id === "gpu")).toBe(false);
    const ids = checkSetup(probe({ gpu: { state: "metal" } })).map((r) => r.id);
    expect(ids.slice(ids.indexOf("speakers"), ids.indexOf("speakers") + 3)).toEqual(["speakers", "gpu", "summary"]);
    expect(find(probe({ gpu: { state: "metal" } }), "gpu")).toMatchObject({ status: "ok", detail: "Apple GPU (Metal)." });
    expect(find(probe({ gpu: { state: "off" } }), "gpu")).toMatchObject({ status: "ok" });
    expect(find(probe({ gpu: { state: "off" } }), "gpu").fix).toBeUndefined();
    expect(find(probe({ gpu: { state: "off" } }), "gpu").detail).toContain("processor");
    const rtx = find(probe({ gpu: { state: "in_use", device: "NVIDIA GeForce RTX 5070 Ti", vramMb: 16303 } }), "gpu");
    expect(rtx.status).toBe("ok");
    expect(rtx.fix).toBeUndefined();
    expect(rtx.detail).toContain("NVIDIA GeForce RTX 5070 Ti (16 GB)");
    expect(find(probe({ gpu: { state: "unavailable", reason: "no_gpu" } }), "gpu").status).toBe("ok");
    for (const s of checkSetup(probe({ gpu: { state: "failed", detail: "speech GPU helper stopped: exit code 5" } }))) expect(s.status).not.toBe("fail");
  });

  it("says why the graphics chip isn't used, with Try again where trying again can help", () => {
    const failed = find(probe({ gpu: { state: "failed", detail: "speech GPU helper stopped: exit code 5" } }), "gpu");
    expect(failed).toMatchObject({ status: "warn", fix: { action: "retryGpu" } });
    expect(failed.detail).toContain("exit code 5");
    expect(find(probe({ gpu: { state: "unavailable", reason: "no_vulkan" } }), "gpu")).toMatchObject({ status: "warn", fix: { action: "retryGpu" } });
    expect(find(probe({ gpu: { state: "unavailable", reason: "no_start", detail: "0xC0000135" } }), "gpu").detail).toContain("0xC0000135");
    expect(find(probe({ gpu: { state: "unavailable", reason: "no_helper" } }), "gpu")).toMatchObject({ status: "warn", fix: undefined });
  });

  it("reports a data folder it can't read", () => {
    expect(find(probe({ data: { error: "permission denied" } }), "data")).toMatchObject({ status: "fail" });
  });
});

describe("checkSetup: folder copies", () => {
  const on = settings({ autoSaveFolder: "/Volumes/Notes/Meetings" });

  it("has no row until a folder is chosen", () => {
    expect(checkSetup(probe()).some((r) => r.id === "autoSave")).toBe(false);
  });

  it("is fine while the folder is there", () => {
    expect(find(probe({ settings: on, autoSave: { reachable: true, pending: [] } }), "autoSave")).toMatchObject({ status: "ok" });
  });

  it("quietly says when the folder is away and how many meetings wait", () => {
    const r = find(probe({ settings: on, autoSave: { reachable: false, pending: [{ id: "a", kind: "missing" }, { id: "b", kind: "waiting" }] } }), "autoSave");
    expect(r).toMatchObject({ status: "warn", fix: { action: "retryAutoSave" } });
    expect(r.detail).toContain("/Volumes/Notes/Meetings");
    expect(r.detail).toContain("2 meetings are waiting");
  });
});

describe("checkSetup: folder copies that failed", () => {
  const on = settings({ autoSaveFolder: "/notes" });

  it("shows any write failure with the count and Try again", () => {
    const r = find(probe({ settings: on, autoSave: { reachable: true, pending: [{ id: "a", kind: "failed", error: "Access is denied." }] } }), "autoSave");
    expect(r).toMatchObject({ status: "warn", fix: { action: "retryAutoSave" } });
    expect(r.detail).toContain("Access is denied.");
    expect(r.detail).toContain("1 meeting is waiting");
  });

  it("never puts the folder or the error's text in the diagnostics", () => {
    const s = settings({ autoSaveFolder: "/Users/miguel/Clients/Acme" });
    for (const autoSave of [
      { reachable: true, pending: [] },
      { reachable: false, pending: [{ id: "a", kind: "missing" }] },
      { reachable: true, pending: [{ id: "a", kind: "failed", error: "Access is denied: /Users/miguel/Clients/Acme/Budget.md" }] },
    ] as AutoSaveStatus[]) {
      const [line] = setupLines([find(probe({ settings: s, autoSave }), "autoSave")]);
      expect(line).toContain("[folder]");
      expect(line).not.toMatch(/Acme|miguel|Budget/);
    }
  });

  it("doesn't count copies that are just on their way", () => {
    expect(find(probe({ settings: on, autoSave: { reachable: true, pending: [{ id: "a", kind: "waiting" }] } }), "autoSave").status).toBe("ok");
  });
});
