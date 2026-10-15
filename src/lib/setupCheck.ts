import type { RecorderClient } from "../data/recorder";
import type { Settings, SettingsStore } from "../data/settings";
import { formatBytes, tildePath } from "./format";

/** `unknown`: can't be told without a test (call audio has no permission to read). */
export type CheckStatus = "ok" | "warn" | "fail" | "unknown";
export type FixAction = "allowMic" | "micSettings" | "testSystemAudio" | "systemAudioSettings" | "speechModels" | "summaryAI" | "openData";

export interface CheckResult {
  id: "mic" | "device" | "systemAudio" | "speech" | "speakers" | "summary" | "data";
  title: string;
  status: CheckStatus;
  detail: string;
  fix?: { label: string; action: FixAction };
}

/** What was found on this computer. Gathered by `probeSetup`, judged by `checkSetup`. */
export interface Probe {
  settings: Settings;
  mic: Awaited<ReturnType<RecorderClient["microphonePermission"]>>;
  /** Connected microphones; undefined when they couldn't be listed. */
  inputs?: string[];
  /** Result of "Test call audio"; undefined until it runs. */
  systemAudio?: boolean;
  /** Ollama / LM Studio, when that is the summary engine. */
  localServer?: { ok: true; models: string[] } | { ok: false; message: string };
  data: { path: string; bytes: number } | { error: string };
}

const READY = new Set(["downloaded", "on_this_mac"]);

/** One line per thing a call needs, in the order a call uses them, each with what to do about it. */
export function checkSetup(p: Probe): CheckResult[] {
  const s = p.settings;
  return [mic(p), device(p), systemAudio(p), speech(s), speakers(s), summary(p), data(p)];
}

function mic(p: Probe): CheckResult {
  const title = "Microphone access";
  switch (p.mic) {
    case "granted":
      return { id: "mic", title, status: "ok", detail: "Allowed. Your side of the call is recorded." };
    case "undetermined":
      return { id: "mic", title, status: "warn", detail: "Not asked yet. Your system asks the first time Talaan records.", fix: { label: "Allow now", action: "allowMic" } };
    case "denied":
    case "restricted":
      return { id: "mic", title, status: "fail", detail: "Turned off for Talaan, so your side of calls isn’t recorded.", fix: { label: "Open system settings", action: "micSettings" } };
    default:
      return { id: "mic", title, status: "warn", detail: "Talaan couldn’t read the microphone permission." };
  }
}

function device(p: Probe): CheckResult {
  const title = "Microphone";
  const chosen = p.settings.microphone !== "default" ? p.settings.microphone : undefined;
  if (!p.inputs) return { id: "device", title, status: "warn", detail: "Talaan couldn’t list the microphones." };
  if (p.inputs.length === 0) return { id: "device", title, status: "fail", detail: "No microphone is connected." };
  if (chosen && !p.inputs.includes(chosen)) {
    return { id: "device", title, status: "warn", detail: `${chosen} isn’t connected, so the system default is used.` };
  }
  return { id: "device", title, status: "ok", detail: chosen ?? `System default · ${p.inputs.length} connected` };
}

function systemAudio(p: Probe): CheckResult {
  const title = "Call audio";
  if (p.systemAudio === undefined) {
    return { id: "systemAudio", title, status: "unknown", detail: "The other people on a call. Test it to make sure Talaan can hear them.", fix: { label: "Test call audio", action: "testSystemAudio" } };
  }
  return p.systemAudio
    ? { id: "systemAudio", title, status: "ok", detail: "Talaan can record the other people on a call." }
    : { id: "systemAudio", title, status: "fail", detail: "Talaan can’t capture call audio, so only your side would be recorded.", fix: { label: "Open system settings", action: "systemAudioSettings" } };
}

function speech(s: Settings): CheckResult {
  const title = "Speech model";
  const fix = { label: "Speech models", action: "speechModels" as const };
  const active = s.speechModels.find((m) => m.id === s.activeSpeechModel) ?? s.speechModels[0];
  if (active && READY.has(active.state)) return { id: "speech", title, status: "ok", detail: `${active.name} is ready.` };
  if (active?.state === "downloading") {
    return { id: "speech", title, status: "warn", detail: `${active.name} is downloading (${Math.round((active.progress ?? 0) * 100)}%). Calls wait for it.` };
  }
  const other = s.speechModels.find((m) => READY.has(m.state));
  if (other) {
    return { id: "speech", title, status: "warn", detail: `${active?.name ?? "The chosen model"} isn’t downloaded. Pick ${other.name}, or download it.`, fix };
  }
  return { id: "speech", title, status: "fail", detail: "No speech model yet, so calls can’t be transcribed.", fix: { label: "Download one", action: "speechModels" } };
}

function speakers(s: Settings): CheckResult {
  const title = "Speaker separation";
  if (!s.separateSpeakers) return { id: "speakers", title, status: "ok", detail: "Off. Everyone else is labeled Them." };
  if (READY.has(s.speakerModel.state)) return { id: "speakers", title, status: "ok", detail: "Ready. Other people are told apart: Speaker 1, 2, 3." };
  return { id: "speakers", title, status: "warn", detail: "Not downloaded, so everyone else is labeled Them. Optional.", fix: { label: "Speech models", action: "speechModels" } };
}

function summary(p: Probe): CheckResult {
  const s = p.settings;
  const title = "Summary AI";
  const fix = { label: "Summary AI", action: "summaryAI" as const };
  const waits = s.summarizeAutomatically ? "" : " Summaries wait for Generate summary.";
  const r = (status: CheckStatus, detail: string, withFix = status !== "ok"): CheckResult => ({ id: "summary", title, status, detail: detail + waits, fix: withFix ? fix : undefined });

  if (s.summaryEngine === "byok") {
    return s.byokKeySet ? r("ok", "Your own key is saved. Only transcript text is sent.") : r("fail", "No API key saved, so calls get a transcript only.");
  }
  if (s.summaryEngine === "ollama") {
    const server = p.localServer;
    if (!server) return r("unknown", `Not checked: ${s.ollamaAddress}.`);
    if (!server.ok) return r("fail", `Can’t reach ${s.ollamaAddress}. Start Ollama or LM Studio. (${server.message})`);
    if (server.models.length === 0) return r("fail", "The local server has no models.");
    if (s.ollamaModel && !server.models.includes(s.ollamaModel)) return r("warn", `${s.ollamaModel} isn’t on the local server any more.`);
    return r("ok", `Local server ready${s.ollamaModel ? ` · ${s.ollamaModel}` : ""}.`);
  }
  const model = s.localModels.find((m) => m.choice === s.localModel) ?? s.localModels[0];
  if (model?.state === "downloaded") return r("ok", `Local AI · ${model.name} is ready.`);
  if (model?.state === "downloading") return r("warn", `${model.name} is downloading (${Math.round((model.progress ?? 0) * 100)}%).`);
  return r("fail", `${model?.name ?? "The local model"} isn’t downloaded, so calls get a transcript only.`);
}

function data(p: Probe): CheckResult {
  const title = "Meetings folder";
  if ("error" in p.data) return { id: "data", title, status: "fail", detail: `Talaan can’t read its folder: ${p.data.error}` };
  return { id: "data", title, status: "ok", detail: `${tildePath(p.data.path)} · ${formatBytes(p.data.bytes)}`, fix: { label: "Open folder", action: "openData" } };
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Looks at this computer. Never prompts: call audio is only tested when the user asks (`testSystemAudio`). */
export async function probeSetup(store: SettingsStore, recorder: RecorderClient, testSystemAudio?: boolean): Promise<Probe> {
  const settings = await store.get();
  const [mic, inputs, data, localServer] = await Promise.all([
    recorder.microphonePermission().catch(() => "unknown" as const),
    store.audioInputs().catch(() => undefined),
    store.dataInfo().catch((e) => ({ error: message(e) })),
    settings.summaryEngine === "ollama"
      ? store.localServerModels(settings.ollamaAddress).then(
          (models) => ({ ok: true as const, models }),
          (e) => ({ ok: false as const, message: message(e) }),
        )
      : Promise.resolve(undefined),
  ]);
  const systemAudio = testSystemAudio ? await recorder.requestSystemAudio().catch(() => false) : undefined;
  return { settings, mic, inputs, data, localServer, systemAudio };
}
