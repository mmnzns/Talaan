import type { SpeechModel } from "../data/settings";

type Model = Pick<SpeechModel, "detectsLanguage" | "languageCodes">;

/** The model can't tell the language by itself (Cohere Transcribe): it is always given one, and Auto means English. */
export const cannotDetectLanguage = (m: Model | undefined): boolean => m?.detectsLanguage === false;

/** The language a model is given for a meeting: what is picked, with Auto turned into English for a model that can't
 *  detect. The same rule as src-tauri/src/transcriber/engine.rs (`effective_language`). */
export function effectiveMeetingLanguage(m: Model | undefined, picked: string): string {
  return picked === "auto" && cannotDetectLanguage(m) ? "en" : picked;
}

/** The languages the meeting language list shows for a model: null means the usual list (the model detects the
 *  language); otherwise only the languages the model writes, plus the current pick when it isn't one of them (so
 *  the list never shows a language the model can't use as if it were fine; the transcription says so). */
export function languagesFor(m: Model | undefined, picked: string): string[] | null {
  if (!cannotDetectLanguage(m)) return null;
  const codes = [...(m?.languageCodes ?? [])];
  return picked !== "auto" && !codes.includes(picked) ? [picked, ...codes] : codes;
}
