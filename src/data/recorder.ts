import { call } from "./tauri/invoke";
import { listen } from "@tauri-apps/api/event";

export interface RecorderState {
  state: "idle" | "recording" | "paused";
  meetingId?: string;
  elapsedMs: number;
  marks: number;
  notes: number;
  micName: string;
  systemAudio: boolean;
  onBuiltInSpeakers: boolean;
  /** Windows: the sound enhancer or mixer the default output goes through ("FxSound"), if a known one. */
  soundRelay?: string | null;
}

export interface RecorderLevels {
  me: number;
  them: number;
}

export interface RecorderNotice {
  text: string;
  kind: "info" | "warning";
  /** System Settings pane that fixes the problem, if any. */
  settingsPane?: "microphone" | "system-audio";
}

export const IDLE: RecorderState = { state: "idle", elapsedMs: 0, marks: 0, notes: 0, micName: "", systemAudio: false, onBuiltInSpeakers: false };

/** The recorder as the UI sees it. Tauri in the app, a simulation in the browser and on the Screens stage. */
export interface RecorderClient {
  status(): Promise<RecorderState>;
  start(app?: { id: string; label: string }): Promise<RecorderState>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  mark(note?: string): Promise<void>;
  /** Returns the id of the saved meeting. */
  stop(): Promise<string>;
  discard(): Promise<void>;
  microphonePermission(): Promise<"granted" | "denied" | "restricted" | "undetermined" | "unknown">;
  requestMicrophone(): Promise<boolean>;
  /** Shows the system-audio prompt (opens the capture for a moment). True when capture could start. */
  requestSystemAudio(): Promise<boolean>;
  openSettings(pane: "microphone" | "system-audio" | "calendar" | "automation"): Promise<void>;
  /** The sound enhancer the default output goes through (Windows), for Check setup. */
  soundRelay(): Promise<string | null>;
  onState(cb: (s: RecorderState) => void): () => void;
  onLevels(cb: (l: RecorderLevels) => void): () => void;
  onNotice(cb: (n: RecorderNotice) => void): () => void;
}

function listenPayload<T>(event: string, cb: (payload: T) => void): () => void {
  const pending = listen<T>(event, (e) => cb(e.payload));
  return () => {
    pending.then((off) => off()).catch(() => {});
  };
}

export class TauriRecorderClient implements RecorderClient {
  status() {
    return call<RecorderState>("recorder_status");
  }
  start(app?: { id: string; label: string }) {
    return call<RecorderState>("recorder_start", { appId: app?.id ?? null, appLabel: app?.label ?? null });
  }
  pause() {
    return call<void>("recorder_pause");
  }
  resume() {
    return call<void>("recorder_resume");
  }
  mark(note?: string) {
    return call<void>("recorder_mark", { note: note ?? null });
  }
  stop() {
    return call<string>("recorder_stop");
  }
  discard() {
    return call<void>("recorder_discard");
  }
  async microphonePermission() {
    return (await call<{ microphone: Awaited<ReturnType<RecorderClient["microphonePermission"]>> }>("permissions_status")).microphone;
  }
  requestMicrophone() {
    return call<boolean>("permissions_request_mic");
  }
  requestSystemAudio() {
    return call<boolean>("permissions_request_system_audio");
  }
  openSettings(pane: "microphone" | "system-audio" | "calendar" | "automation") {
    return call<void>("permissions_open_settings", { pane });
  }
  soundRelay() {
    return call<string | null>("audio_sound_relay");
  }
  onState(cb: (s: RecorderState) => void) {
    return listenPayload<RecorderState>("recorder-state", cb);
  }
  onLevels(cb: (l: RecorderLevels) => void) {
    return listenPayload<RecorderLevels>("recorder-levels", cb);
  }
  onNotice(cb: (n: RecorderNotice) => void) {
    return listenPayload<RecorderNotice>("recorder-notice", cb);
  }
}

/** A believable recording with no audio: timer, levels, pause, marks. */
export class MockRecorderClient implements RecorderClient {
  private s: RecorderState = { ...IDLE };
  private stateCbs = new Set<(s: RecorderState) => void>();
  private levelCbs = new Set<(l: RecorderLevels) => void>();
  private noticeCbs = new Set<(n: RecorderNotice) => void>();
  private timers: ReturnType<typeof setInterval>[] = [];

  constructor(private opts: { startAtMs?: number; speakers?: boolean; relay?: string } = {}) {}

  private emit() {
    const snapshot = { ...this.s };
    for (const cb of this.stateCbs) cb(snapshot);
  }

  private clear() {
    this.timers.forEach(clearInterval);
    this.timers = [];
  }

  async status() {
    return { ...this.s };
  }

  async start() {
    this.clear();
    this.s = { state: "recording", meetingId: "", elapsedMs: this.opts.startAtMs ?? 0, marks: 0, notes: 0, micName: "AirPods Pro", systemAudio: true, onBuiltInSpeakers: this.opts.speakers ?? true, soundRelay: this.opts.relay ?? null };
    this.timers.push(
      setInterval(() => {
        if (this.s.state === "recording") {
          this.s.elapsedMs += 1000;
          this.emit();
        }
      }, 1000),
      setInterval(() => {
        const live = this.s.state === "recording";
        const l = { me: live ? Math.random() * 0.7 : 0, them: live ? 0.2 + Math.random() * 0.7 : 0 };
        for (const cb of this.levelCbs) cb(l);
      }, 150),
    );
    this.emit();
    return { ...this.s };
  }

  async pause() {
    this.s.state = "paused";
    this.emit();
  }

  async resume() {
    this.s.state = "recording";
    this.emit();
  }

  async mark(note?: string) {
    this.s.marks += 1;
    if (note?.trim()) this.s.notes += 1;
    this.emit();
  }

  async stop() {
    this.clear();
    const id = this.s.meetingId ?? "";
    this.s = { ...IDLE };
    this.emit();
    return id;
  }

  async discard() {
    await this.stop();
  }

  async microphonePermission() {
    return "granted" as const;
  }

  async requestMicrophone() {
    return true;
  }

  async requestSystemAudio() {
    return true;
  }

  async openSettings() {}

  async soundRelay() {
    return this.s.soundRelay ?? this.opts.relay ?? null;
  }

  /** Stage helper: shows a notice as if it came from the backend. */
  notice(n: RecorderNotice) {
    for (const cb of this.noticeCbs) cb(n);
  }

  setSpeakers(on: boolean) {
    this.s.onBuiltInSpeakers = on;
    this.emit();
  }

  /** Stage helper: the default output goes through a sound enhancer. */
  setRelay(name: string | null) {
    this.s.soundRelay = name;
    this.emit();
  }

  onState(cb: (s: RecorderState) => void) {
    this.stateCbs.add(cb);
    return () => this.stateCbs.delete(cb);
  }
  onLevels(cb: (l: RecorderLevels) => void) {
    this.levelCbs.add(cb);
    return () => this.levelCbs.delete(cb);
  }
  onNotice(cb: (n: RecorderNotice) => void) {
    this.noticeCbs.add(cb);
    return () => this.noticeCbs.delete(cb);
  }
}

