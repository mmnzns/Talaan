import type { RecorderClient } from "../data/recorder";
import { hasByokKey, type AutoSaveStatus, type CalendarAccess, type GpuStatus, type Settings, type SettingsStore } from "../data/settings";
import { formatBytes, tildePath } from "./format";
import { currentLang, t } from "./i18n";
import { onWindows, os, platformCopy } from "./platform";

/** `unknown`: can't be told without a test (call audio has no permission to read). */
export type CheckStatus = "ok" | "warn" | "fail" | "unknown";
export type FixAction = "allowMic" | "micSettings" | "testSystemAudio" | "systemAudioSettings" | "speechModels" | "summaryAI" | "connections" | "openData" | "retryGpu" | "retryAutoSave";

export interface CheckResult {
  id: "mic" | "device" | "systemAudio" | "relay" | "speech" | "speakers" | "gpu" | "summary" | "calendar" | "data" | "autoSave";
  title: string;
  status: CheckStatus;
  detail: string;
  /** What "Copy diagnostics" puts in the report instead of `detail`, when `detail` holds a path. */
  reportDetail?: string;
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
  /** macOS asks for call audio; Windows loopback needs no permission, so there is nothing to open. Default true. */
  systemAudioNeedsPermission?: boolean;
  /** Windows, calendar on: whether its iCal address is in secure key storage ("allowed"). Undefined = no row. */
  calendar?: CalendarAccess;
  /** Whether the GPU is used. Undefined (couldn't be asked) = no row. */
  gpu?: GpuStatus;
  /** Windows: the sound enhancer the default output goes through ("FxSound"). None = no row. */
  soundRelay?: string | null;
  /** "Also save each meeting to a folder", when a folder is chosen. Undefined = no row. */
  autoSave?: AutoSaveStatus;
}

const READY = new Set(["downloaded"]);
const openSettings = () => platformCopy(os, currentLang()).openSettings;
const pct = (p?: number) => Math.round((p ?? 0) * 100);

/** One line per thing a call needs, in the order a call uses them, each with what to do about it. */
export function checkSetup(p: Probe): CheckResult[] {
  const s = p.settings;
  return [mic(p), device(p), systemAudio(p), ...relay(p), speech(s), speakers(s), ...gpu(p), summary(p), ...calendar(p), data(p), ...autoSave(p)];
}

/** The checks the Settings button's dot is for: only what stops a call from being recorded or transcribed
 *  ("fail"). The summary check never lights it: a transcript-only setup is a valid choice, and an Ollama server
 *  that's off is normal; summary problems show on the meeting and in Status. Notices (a sound enhancer, the
 *  processor instead of the graphics card, a folder that is away) and checks that need a test ("unknown") never
 *  light it either. */
export function needsFixing(results: CheckResult[]): CheckResult[] {
  return results.filter((r) => r.status === "fail" && r.id !== "summary");
}

/** Windows: a sound enhancer or mixer replays everything, Talaan's own sounds included, under its own program. */
function relay(p: Probe): CheckResult[] {
  if (!p.soundRelay) return [];
  return [{ id: "relay", title: t("settings.setup_relay"), status: "warn", detail: t("settings.setup_relay_found", { name: p.soundRelay }) }];
}

/** Windows only (the Mac's calendar is a system permission, asked in Connections): the saved iCal address. */
function calendar(p: Probe): CheckResult[] {
  if (p.calendar === undefined) return [];
  const title = t("settings.calendar_title");
  if (p.calendar === "allowed") return [{ id: "calendar", title, status: "ok", detail: t("settings.setup_calendar_ok") }];
  return [{ id: "calendar", title, status: "warn", detail: t("settings.setup_calendar_missing"), fix: { label: t("settings.tab_connections"), action: "connections" } }];
}

/** Speech and local summaries on the graphics chip. Not using it is never an error: Talaan uses the processor. */
function gpu(p: Probe): CheckResult[] {
  const g = p.gpu;
  if (!g) return [];
  const title = t("settings.setup_gpu");
  const ok = (detail: string): CheckResult[] => [{ id: "gpu", title, status: "ok", detail }];
  const warn = (detail: string): CheckResult[] => [{ id: "gpu", title, status: "warn", detail, fix: { label: t("settings.setup_gpu_retry"), action: "retryGpu" } }];
  const error = g.detail ?? "?";
  switch (g.state) {
    case "metal":
      return ok(t("settings.setup_gpu_metal"));
    case "off":
      return ok(t("settings.setup_gpu_off"));
    case "in_use": {
      const name = g.device ?? "GPU";
      return ok(g.vramMb ? t("settings.setup_gpu_in_use", { name, size: Math.round(g.vramMb / 1024) }) : t("settings.setup_gpu_in_use_plain", { name }));
    }
    case "failed":
      return warn(t("settings.setup_gpu_failed", { error }));
    case "unavailable":
      return ok(t("settings.setup_gpu_no_gpu"));
  }
}

function mic(p: Pick<Probe, "mic">): CheckResult {
  const title = t("settings.setup_mic");
  switch (p.mic) {
    case "granted":
      return { id: "mic", title, status: "ok", detail: t("settings.setup_mic_ok") };
    case "undetermined":
      return { id: "mic", title, status: "warn", detail: t("settings.setup_mic_ask"), fix: { label: t("settings.setup_mic_allow"), action: "allowMic" } };
    case "denied":
    case "restricted":
      return { id: "mic", title, status: "fail", detail: t("settings.setup_mic_off"), fix: { label: openSettings(), action: "micSettings" } };
    default:
      return { id: "mic", title, status: "warn", detail: t("settings.setup_mic_unknown") };
  }
}

function device(p: Pick<Probe, "settings" | "inputs">): CheckResult {
  const title = t("settings.setup_device");
  const chosen = p.settings.microphone !== "default" ? p.settings.microphone : undefined;
  if (!p.inputs) return { id: "device", title, status: "warn", detail: t("settings.setup_device_unknown") };
  if (p.inputs.length === 0) return { id: "device", title, status: "fail", detail: t("settings.setup_device_none") };
  if (chosen && !p.inputs.includes(chosen)) return { id: "device", title, status: "warn", detail: t("settings.setup_device_gone", { name: chosen }) };
  return { id: "device", title, status: "ok", detail: chosen ?? t("settings.setup_device_default", { count: p.inputs.length }) };
}

function systemAudio(p: Probe): CheckResult {
  const title = t("settings.setup_audio");
  if (p.systemAudio === undefined) {
    return { id: "systemAudio", title, status: "unknown", detail: t("settings.setup_audio_untested"), fix: { label: t("settings.setup_audio_test"), action: "testSystemAudio" } };
  }
  if (p.systemAudio) return { id: "systemAudio", title, status: "ok", detail: t("settings.setup_audio_ok") };
  const fix = p.systemAudioNeedsPermission === false ? undefined : { label: openSettings(), action: "systemAudioSettings" as const };
  return { id: "systemAudio", title, status: "fail", detail: t("settings.setup_audio_fail"), fix };
}

function speech(s: Settings): CheckResult {
  const title = t("settings.setup_speech");
  const fix = { label: t("settings.tab_models"), action: "speechModels" as const };
  const active = s.speechModels.find((m) => m.id === s.activeSpeechModel) ?? s.speechModels[0];
  if (active && READY.has(active.state)) return { id: "speech", title, status: "ok", detail: t("settings.setup_ready", { name: active.name }) };
  if (active?.state === "downloading") {
    return { id: "speech", title, status: "warn", detail: t("settings.setup_speech_downloading", { name: active.name, pct: pct(active.progress) }) };
  }
  const other = s.speechModels.find((m) => READY.has(m.state));
  if (other) return { id: "speech", title, status: "warn", detail: t("settings.setup_speech_pick", { name: active?.name ?? "?", other: other.name }), fix };
  return { id: "speech", title, status: "fail", detail: t("settings.setup_speech_none"), fix };
}

function speakers(s: Settings): CheckResult {
  const title = t("settings.split_title");
  if (!s.separateSpeakers) return { id: "speakers", title, status: "ok", detail: t("settings.setup_speakers_off") };
  if (READY.has(s.speakerModel.state)) return { id: "speakers", title, status: "ok", detail: t("settings.setup_speakers_ok") };
  return { id: "speakers", title, status: "warn", detail: t("settings.setup_speakers_missing"), fix: { label: t("settings.tab_models"), action: "speechModels" } };
}

function summary(p: Probe): CheckResult {
  const s = p.settings;
  const title = t("settings.tab_ai");
  const fix = { label: t("settings.tab_ai"), action: "summaryAI" as const };
  const waits = s.summarizeAutomatically ? "" : ` ${t("settings.setup_summary_waits")}`;
  const r = (status: CheckStatus, detail: string): CheckResult => ({ id: "summary", title, status, detail: detail + waits, fix: status !== "ok" ? fix : undefined });

  // A local OpenAI-compatible server may need no key.
  if (s.summaryEngine === "byok") return hasByokKey(s) || s.byokProvider === "compatible" ? r("ok", t("settings.setup_summary_key")) : r("fail", t("settings.setup_summary_nokey"));
  if (s.summaryEngine === "ollama") {
    const server = p.localServer;
    if (!server) return r("unknown", t("settings.setup_summary_unchecked", { address: s.ollamaAddress }));
    if (!server.ok) return r("fail", t("settings.setup_summary_unreachable", { address: s.ollamaAddress, error: server.message }));
    if (server.models.length === 0) return r("fail", t("settings.setup_summary_nomodels"));
    if (s.ollamaModel && !server.models.includes(s.ollamaModel)) return r("warn", t("settings.setup_summary_gone", { name: s.ollamaModel }));
    return r("ok", s.ollamaModel ? t("settings.setup_summary_server_model", { name: s.ollamaModel }) : t("settings.setup_summary_server"));
  }
  const model = s.localModels.find((m) => m.id === s.localModel) ?? s.localModels[0];
  if (model?.state === "downloaded") return r("ok", t("settings.setup_ready", { name: `${t("settings.engine_local")} · ${model.name}` }));
  if (model?.state === "downloading") return r("warn", t("settings.setup_summary_downloading", { name: model.name, pct: pct(model.progress) }));
  return r("fail", t("settings.setup_summary_nolocal", { name: model?.name ?? "?" }));
}

/** The folder copies: a quiet notice when the folder can't be reached (a disconnected drive), never an error. */
function autoSave(p: Probe): CheckResult[] {
  const a = p.autoSave;
  if (!a) return [];
  const title = t("settings.setup_autosave");
  const shown = tildePath(p.settings.autoSaveFolder);
  // "waiting" copies are on their way; only ones that didn't make it are a problem.
  const stuck = a.pending.filter((p) => p.kind !== "waiting");
  // The report never says where the folder is: its path can hold the user's name or a client's.
  const both = (make: (path: string, error: string) => string) => ({ detail: make(shown, failed?.error ?? "?"), reportDetail: make("[folder]", "?") });
  const failed = stuck.find((p) => p.kind === "failed");
  if (a.reachable && stuck.length === 0) return [{ id: "autoSave", title, status: "ok", ...both((path) => t("settings.setup_autosave_ok", { path })) }];
  const count = a.reachable ? stuck.length : a.pending.length;
  const waiting = count ? ` ${t("settings.setup_autosave_skipped", { count })}` : "";
  const why = (path: string, error: string) =>
    !a.reachable
      ? t("settings.setup_autosave_missing", { path })
      : failed
        ? t("settings.setup_autosave_failed", { path, error })
        : t("settings.setup_autosave_ok", { path });
  return [{ id: "autoSave", title, status: "warn", ...both((path, error) => why(path, error) + waiting), fix: { label: t("settings.setup_autosave_retry"), action: "retryAutoSave" } }];
}

function data(p: Probe): CheckResult {
  const title = t("settings.setup_data");
  if ("error" in p.data) return { id: "data", title, status: "fail", detail: t("settings.setup_data_fail", { error: p.data.error }) };
  return { id: "data", title, status: "ok", detail: `${tildePath(p.data.path)} · ${formatBytes(p.data.bytes)}`, fix: { label: t("settings.folder_open"), action: "openData" } };
}

/** The last full look with the graphics card asked again (after its switch changes): every other row stays as it
 *  was, so nothing else is probed. */
export async function regraphics(p: Probe, store: Pick<SettingsStore, "get" | "gpuStatus">): Promise<Probe> {
  const [settings, gpu] = await Promise.all([store.get(), store.gpuStatus().catch(() => undefined)]);
  return { ...p, settings, gpu };
}

/** What the Settings button's dot looks at: microphone access, the microphones connected and the speech model
 *  from settings. Cheap and quiet on purpose: no helper starts (graphics card), no folder is walked (data size,
 *  auto-save) and nothing goes over the network (a local AI server, the calendar). The full look is `probeSetup`,
 *  for the Status tab. */
export type LightProbe = Pick<Probe, "settings" | "mic" | "inputs">;

export async function probeLight(store: Pick<SettingsStore, "get" | "audioInputs">, recorder: Pick<RecorderClient, "microphonePermission">): Promise<LightProbe> {
  const [settings, mic, inputs] = await Promise.all([
    store.get(),
    recorder.microphonePermission().catch(() => "unknown" as const),
    store.audioInputs().catch(() => undefined),
  ]);
  return { settings, mic, inputs };
}

/** The checks a light look can judge, in Status order. */
export function lightChecks(p: LightProbe): CheckResult[] {
  return [mic(p), device(p), speech(p.settings)];
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Looks at this computer. Never prompts: call audio is only tested when the user asks (`testSystemAudio`). */
/** `retryGpu`: forget a remembered GPU failure first (Try again on the Graphics chip row). */
export async function probeSetup(store: SettingsStore, recorder: RecorderClient, testSystemAudio?: boolean, retryGpu?: boolean): Promise<Probe> {
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
  const calendar = onWindows && settings.calendarConnected ? await store.calendarAccess().catch(() => undefined) : undefined;
  const gpu = await (retryGpu ? store.retryGpu() : store.gpuStatus()).catch(() => undefined);
  const soundRelay = await recorder.soundRelay().catch(() => null);
  const autoSave = settings.autoSaveFolder ? ((await store.autoSaveStatus().catch(() => null)) ?? undefined) : undefined;
  return { settings, mic, inputs, data, localServer, systemAudio, systemAudioNeedsPermission: os === "mac", calendar, gpu, soundRelay, autoSave };
}
