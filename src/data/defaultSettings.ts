import { onWindows } from "../lib/platform";
import type { CallAppRule, IgnoredApp, Settings } from "./settings";

/** Windows only: the extra ignored apps the user sees and edits (the "why" shows through settings.why_*).
 *  The Rust side already ignores these ids on Windows (calls/apps.rs). */
const windowsIgnoredApps = onWindows
  ? [
      { name: "NVIDIA Broadcast", why: "mic filter" },
      { name: "SteelSeries Sonar", why: "mic filter" },
      { name: "Voicemod", why: "mic filter" },
      { name: "Krisp", why: "mic filter" },
      { name: "Windows voice typing", why: "built in" },
      { name: "Sound Recorder", why: "built in" },
    ]
  : [];

/** Values used until the user changes something. Shared by the mock and the real settings store. */
export const defaultSettings: Settings = {
  theme: "system",
  yourName: "",
  detectCalls: true,
  autoTagApps: false,
  launchAtLogin: true,
  checkUpdates: true,
  recordingNotice: "Heads up: I'm recording this call for my own notes. It stays on my computer.",
  microphone: "default",
  warnOnSpeakers: true,
  notifySummaryReady: true,
  callApps: [
    { id: "zoom", name: "Zoom", note: "zoom.us app", rule: "ask" },
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
    // Windows extras. Removing a row in Settings doesn't un-ignore the app: the Rust IGNORED table still applies
    // (same as on the Mac for the rows above).
    ...windowsIgnoredApps,
  ],
  speechModels: [
    // Mirrors the engine catalog (voice-engine v0.3.3, crates/models/src/catalog.rs), in the order of src-tauri/src/transcriber/models.rs.
    // In the app the list and states come from Rust.
    { id: "whisper-turbo", name: "Whisper Large Turbo (compressed)", maker: "OpenAI", note: "Most accurate. 99 languages, including Filipino.", engine: "whisper", sizeMb: 575, speed: 2, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "downloaded" },
    { id: "parakeet-v3", name: "Parakeet", maker: "NVIDIA", note: "Good for quick notes in 25 European languages.", engine: "parakeet", sizeMb: 671, speed: 3, accuracy: 1, languages: "25 European languages", licence: "CC-BY-4.0", englishOnly: false, state: "downloaded" },
    { id: "whisper-small", name: "Whisper Small", maker: "OpenAI", note: "A smaller download that works on older computers. Lower accuracy.", engine: "whisper", sizeMb: 488, speed: 3, accuracy: 1, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-turbo-full", name: "Whisper Large Turbo (full size)", maker: "OpenAI", note: "The full-size version: the same words, nearly three times the download, no faster.", engine: "whisper", sizeMb: 1625, speed: 2, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "parakeet-v3-full", name: "Parakeet (full size)", maker: "NVIDIA", note: "The full-size version: the same words, nearly four times the download, no faster.", engine: "parakeet", sizeMb: 2550, speed: 3, accuracy: 1, languages: "25 European languages", licence: "CC-BY-4.0", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-small-en", name: "Whisper Small (English)", maker: "OpenAI", note: "A little more accurate in English than Whisper Small.", engine: "whisper", sizeMb: 488, speed: 3, accuracy: 1, languages: "English", licence: "MIT", englishOnly: true, state: "not_downloaded" },
    { id: "whisper-large-v3", name: "Whisper Large v3 (compressed)", maker: "OpenAI", note: "Whisper’s largest model. Very accurate, slower than Turbo.", engine: "whisper", sizeMb: 1082, speed: 1, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-large-v3-full", name: "Whisper Large v3 (full size)", maker: "OpenAI", note: "The full-size version: the same words, nearly three times the download, no faster.", engine: "whisper", sizeMb: 3096, speed: 1, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-large-v2", name: "Whisper Large v2 (compressed)", maker: "OpenAI", note: "The earlier large model. Some voices and accents come out better.", engine: "whisper", sizeMb: 1081, speed: 1, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-large-v2-full", name: "Whisper Large v2 (full size)", maker: "OpenAI", note: "The full-size version: the same words, nearly three times the download, no faster.", engine: "whisper", sizeMb: 3095, speed: 1, accuracy: 3, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-medium", name: "Whisper Medium", maker: "OpenAI", note: "Between Small and Large: good accuracy at a moderate speed.", engine: "whisper", sizeMb: 1534, speed: 1, accuracy: 2, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-medium-en", name: "Whisper Medium (English)", maker: "OpenAI", note: "A little more accurate in English than Whisper Medium.", engine: "whisper", sizeMb: 1534, speed: 1, accuracy: 2, languages: "English", licence: "MIT", englishOnly: true, state: "not_downloaded" },
    { id: "whisper-base", name: "Whisper Base", maker: "OpenAI", note: "Very small and fast. Rough on names and accents.", engine: "whisper", sizeMb: 148, speed: 3, accuracy: 1, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-base-en", name: "Whisper Base (English)", maker: "OpenAI", note: "A little more accurate in English than Whisper Base.", engine: "whisper", sizeMb: 148, speed: 3, accuracy: 1, languages: "English", licence: "MIT", englishOnly: true, state: "not_downloaded" },
    { id: "whisper-tiny", name: "Whisper Tiny", maker: "OpenAI", note: "The smallest. For trying Talaan out or very old computers.", engine: "whisper", sizeMb: 78, speed: 3, accuracy: 1, languages: "99 languages", licence: "MIT", englishOnly: false, state: "not_downloaded" },
    { id: "whisper-tiny-en", name: "Whisper Tiny (English)", maker: "OpenAI", note: "A little more accurate in English than Whisper Tiny.", engine: "whisper", sizeMb: 78, speed: 3, accuracy: 1, languages: "English", licence: "MIT", englishOnly: true, state: "not_downloaded" },
    { id: "parakeet-ultra", name: "Parakeet Ultra", maker: "NVIDIA", note: "Parakeet v3 further trained by Moondream for better accuracy. 25 European languages.", engine: "parakeet", sizeMb: 668, speed: 3, accuracy: 2, languages: "25 European languages", licence: "CC-BY-4.0", englishOnly: false, state: "not_downloaded" },
    { id: "parakeet-v2", name: "Parakeet v2", maker: "NVIDIA", note: "Fast and very accurate in English.", engine: "parakeet", sizeMb: 662, speed: 3, accuracy: 2, languages: "English", licence: "CC-BY-4.0", englishOnly: true, state: "not_downloaded" },
    { id: "parakeet-v2-full", name: "Parakeet v2 (full size)", maker: "NVIDIA", note: "The full-size version: the same words, nearly four times the download, no faster.", engine: "parakeet", sizeMb: 2514, speed: 3, accuracy: 2, languages: "English", licence: "CC-BY-4.0", englishOnly: true, state: "not_downloaded" },
    { id: "parakeet-unified-en", name: "Parakeet Unified", maker: "NVIDIA", note: "NVIDIA’s newer English model, with punctuation.", engine: "parakeet", sizeMb: 664, speed: 3, accuracy: 2, languages: "English", licence: "CC-BY-4.0", englishOnly: true, state: "not_downloaded" },
    { id: "parakeet-unified-en-full", name: "Parakeet Unified (full size)", maker: "NVIDIA", note: "The full-size version: the same words, nearly four times the download, no faster.", engine: "parakeet", sizeMb: 2516, speed: 3, accuracy: 2, languages: "English", licence: "CC-BY-4.0", englishOnly: true, state: "not_downloaded" },
    { id: "nemotron-3.5", name: "Nemotron 3.5", maker: "NVIDIA", note: "NVIDIA’s newest, 35 languages. A larger download.", engine: "parakeet", sizeMb: 2595, speed: 2, accuracy: 2, languages: "35 languages", licence: "OpenMDW-1.1", englishOnly: false, state: "not_downloaded" },
  ],
  activeSpeechModel: "whisper-turbo",
  speakerModel: { id: "diarization", name: "Speaker separation", maker: "NVIDIA", note: "Tells the other people on a call apart: Speaker 1, 2, 3. Up to 8 people.", engine: "diarization", sizeMb: 401, speed: 3, accuracy: 2, languages: "Any language", licence: "OpenMDW-1.1", englishOnly: false, state: "not_downloaded" },
  separateSpeakers: true,
  searchModel: { id: "search", name: "Search by meaning", maker: "Qwen", note: "Lets Ask find answers phrased differently from the question, in 100+ languages.", engine: "search", sizeMb: 640, speed: 3, accuracy: 2, languages: "100+ languages", licence: "Apache-2.0", englishOnly: false, state: "not_downloaded" },
  meetingLanguage: "auto",
  uiLanguage: "auto",
  useGpu: true,
  summaryEngine: "local",
  summarizeAutomatically: true,
  summaryLanguage: "meeting",
  summaryTemplate: "general",
  summaryInstructions: "",
  localModel: "qwen3.5-4b",
  // Mirrors the engine catalog (the local AI models). In the app the states come from disk.
  localModels: [
    { id: "qwen3.5-4b", name: "Qwen3.5 4B", note: "Recommended. Accurate decisions and action items, about 30 s for a short call.", sizeMb: 2741, recommended: true, licence: "Apache-2.0", memoryGb: 3.3, state: "downloaded" },
    { id: "qwen3.5-9b", name: "Qwen3.5 9B", note: "Larger. The most complete action items; needs 16 GB of memory and takes longer.", sizeMb: 5681, recommended: false, licence: "Apache-2.0", memoryGb: 6.8, state: "not_downloaded" },
    { id: "granite-4.2-3b", name: "Granite 4.2 3B", note: "A smaller model from IBM: a smaller download and less memory, for computers that struggle with the others.", sizeMb: 2245, recommended: false, licence: "Apache-2.0", memoryGb: 2.7, state: "not_downloaded" },
  ],
  ollamaAddress: "http://localhost:11434",
  byokProvider: "claude",
  byokKeys: [],
  byokModel: "recommended",
  calendarConnected: false,
  tabDetection: false,
  suggestSpeakerNames: true,
  notion: { connected: false },
  autoSaveFolder: "",
  autoSaveTranscript: false,
  retention: "7d",
  transcriptRetention: "forever",
  onboardingDone: true,
};

/** Apps that only exist on the Mac. They stay in settings (Mac behaviour and rules are unchanged) but Settings ›
 *  Call apps doesn't list them on Windows, where they can't run. */
const MAC_ONLY_CALL_APPS = new Set(["facetime"]);
const MAC_ONLY_IGNORED_APPS = new Set(["MacWhisper", "Apple Dictation", "Voice Memos"]);

/** The call apps to list on this computer. */
export function shownCallApps(apps: CallAppRule[], windows = onWindows): CallAppRule[] {
  return windows ? apps.filter((a) => !MAC_ONLY_CALL_APPS.has(a.id)) : apps;
}

/** The ignored apps to list on this computer. */
export function shownIgnoredApps(apps: IgnoredApp[], windows = onWindows): IgnoredApp[] {
  return windows ? apps.filter((a) => !MAC_ONLY_IGNORED_APPS.has(a.name)) : apps;
}

/** Stored settings over the defaults. Lists that later versions add to (call apps) keep the user's entries and
 *  gain the new defaults, so a new call app shows up without resetting anyone's rules. */
export function withDefaults(stored: Partial<Settings>): Settings {
  // Windows test builds kept the secret calendar address here in plain text (now in secure key storage, see
  // storage/settings.rs). Dropped so update() never writes it back.
  // 1.0.0 kept one "a key is saved" flag for whichever provider was chosen; now each provider has its own.
  const { calendarIcsUrl: _legacy, byokKeySet, ...rest } = stored as Partial<Settings> & { calendarIcsUrl?: unknown; byokKeySet?: boolean };
  const merged = { ...structuredClone(defaultSettings), ...rest };
  if (byokKeySet && !stored.byokKeys) merged.byokKeys = [stored.byokProvider ?? defaultSettings.byokProvider];
  if (stored.callApps) {
    const known = new Set(stored.callApps.map((a) => a.id));
    merged.callApps = [...stored.callApps, ...defaultSettings.callApps.filter((a) => !known.has(a.id))];
  }
  return merged;
}
