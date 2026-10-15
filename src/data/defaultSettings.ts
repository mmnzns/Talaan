import type { Settings } from "./settings";

/** Values used until the user changes something. Shared by the mock and the real settings store. */
export const defaultSettings: Settings = {
  theme: "system",
  detectCalls: true,
  launchAtLogin: true,
  recordingNotice: "Heads up: I'm recording this call for my own notes. It stays on my computer.",
  microphone: "default",
  warnOnSpeakers: true,
  notifySummaryReady: true,
  callApps: [
    { id: "zoom", name: "Zoom", note: "zoom.us app", rule: "always" },
    { id: "slack", name: "Slack", note: "Huddles and calls", rule: "ask" },
    { id: "teams", name: "Microsoft Teams", note: "Teams app", rule: "ask" },
    {
      id: "browser",
      name: "Browser calls",
      note: "Google Meet, Zoom web, Teams web. Always needs browser detection (Connections)",
      rule: "ask",
      isBrowser: true,
    },
    { id: "facetime", name: "FaceTime", note: "Personal calls", rule: "never" },
    { id: "webex", name: "Webex", note: "Webex app", rule: "ask" },
    { id: "discord", name: "Discord", note: "Voice channels and calls", rule: "ask" },
    { id: "whatsapp", name: "WhatsApp", note: "Voice and video calls", rule: "ask" },
  ],
  ignoredApps: [
    { name: "Parlato", why: "dictation" },
    { name: "VoiceInk", why: "dictation" },
    { name: "Superwhisper", why: "dictation" },
    { name: "Wispr Flow", why: "dictation" },
    { name: "MacWhisper", why: "transcription" },
    { name: "Apple Dictation", why: "built in" },
    { name: "Voice Memos", why: "built in" },
  ],
  speechModels: [
    // Mirrors the Rust catalog (src-tauri/src/transcriber/models.rs). In the app the list and states come from Rust.
    { id: "whisper-turbo", name: "Whisper Large Turbo", maker: "OpenAI", note: "Recommended. Most accurate, 99 languages including Filipino.", engine: "whisper", sizeMb: 575, speed: 2, accuracy: 3, languages: "99 languages", state: "downloaded" },
    { id: "parakeet-v3", name: "Parakeet", maker: "NVIDIA", note: "Fastest. Good for quick notes in 25 European languages.", engine: "parakeet", sizeMb: 671, speed: 3, accuracy: 1, languages: "25 European languages", state: "on_this_mac" },
    { id: "whisper-small", name: "Whisper Small", maker: "OpenAI", note: "Light and quick on older Macs. Lower accuracy.", engine: "whisper", sizeMb: 488, speed: 3, accuracy: 1, languages: "99 languages", state: "not_downloaded" },
  ],
  activeSpeechModel: "whisper-turbo",
  speakerModel: { id: "diarization", name: "Speaker separation", maker: "NVIDIA", note: "Tells the other people on a call apart: Speaker 1, 2, 3. Up to 8 people.", engine: "diarization", sizeMb: 401, speed: 3, accuracy: 2, languages: "Any language", state: "not_downloaded" },
  separateSpeakers: true,
  reuseModels: true,
  meetingLanguage: "auto",
  useGpu: true,
  summaryEngine: "local",
  summaryLanguage: "meeting",
  localModel: "recommended",
  // Mirrors the Rust catalog (src-tauri/src/summarizer/models.rs). In the app the states come from disk.
  localModels: [
    { id: "qwen3.5-4b", choice: "recommended", name: "Qwen3.5 4B", note: "Recommended. Accurate decisions and action items, about 30 s for a short call.", sizeMb: 2741, state: "downloaded" },
    { id: "qwen3.5-9b", choice: "larger", name: "Qwen3.5 9B", note: "Larger. The most complete action items; needs 16 GB of memory and takes longer.", sizeMb: 5681, state: "not_downloaded" },
  ],
  ollamaAddress: "http://localhost:11434",
  byokProvider: "claude",
  byokKeySet: false,
  byokModel: "recommended",
  calendarConnected: true,
  tabDetection: true,
  browsers: [
    { name: "Chrome", state: "allowed" },
    { name: "Arc", state: "allowed" },
    { name: "Safari", state: "not_asked" },
    { name: "Edge", state: "not_installed" },
  ],
  suggestSpeakerNames: true,
  notion: { connected: true, workspace: "Miguel", destination: "Meeting Notes (database)" },
  retention: "7d",
  transcriptRetention: "forever",
  onboardingDone: true,
};

/** Stored settings over the defaults. Lists that later versions add to (call apps) keep the user's entries and
 *  gain the new defaults, so a new call app shows up without resetting anyone's rules. */
export function withDefaults(stored: Partial<Settings>): Settings {
  const merged = { ...structuredClone(defaultSettings), ...stored };
  if (stored.callApps) {
    const known = new Set(stored.callApps.map((a) => a.id));
    merged.callApps = [...stored.callApps, ...defaultSettings.callApps.filter((a) => !known.has(a.id))];
  }
  return merged;
}
