import list from "./summaryLanguages.json";

/** One language a summary can be written in. The names come from the platform (`languageNames`), so adding one means
 * adding a code to summaryLanguages.json, which the Rust side reads too. */
export interface SummaryLanguage {
  /** ISO 639-1, plus zh-Hans and zh-Hant; fil is Filipino. */
  code: string;
}

/** The languages, in the order the pickers list them. Same file as src-tauri/src/summarizer/prompt.rs. */
export const SUMMARY_LANGUAGES: readonly SummaryLanguage[] = list.map(({ code }) => ({ code }));

const known = new Set(SUMMARY_LANGUAGES.map((l) => l.code));

export const isSummaryLanguage = (code: string | undefined): code is string => code !== undefined && known.has(code);

/** What the speech model reports and older settings hold, mapped onto the list ("tl" is Filipino, "en-US" is English). */
export function normalizeLanguage(code: string | undefined): string | undefined {
  if (!code) return undefined;
  if (known.has(code)) return code;
  const aliases: Record<string, string> = { tl: "fil", zh: "zh-Hans", nb: "no", nn: "no", iw: "he", in: "id" };
  const base = code.split(/[-_]/)[0].toLowerCase();
  const found = aliases[code] ?? aliases[base] ?? base;
  if (found === "zh") {
    return /hant|tw|hk|mo/i.test(code) ? "zh-Hant" : "zh-Hans";
  }
  return known.has(found) ? found : undefined;
}

const displayName = (code: string, inLanguage: string): string => {
  try {
    return new Intl.DisplayNames([inLanguage], { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
};

/** A language's name in the app's language, and in its own. */
export function languageNames(code: string, ui: string): { name: string; native: string } {
  if (!code) return { name: "", native: "" };
  return { name: displayName(code, ui), native: displayName(code, code) };
}

/** "Japanese · 日本語", or just the name when both are the same ("English"). */
export function languageLabel(code: string, ui: string): string {
  const { name, native } = languageNames(code, ui);
  return native && native.toLowerCase() !== name.toLowerCase() ? `${name} · ${native}` : name;
}

const plain = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();

/** The languages whose name (in the app's language or its own) contains the search, ignoring case and accents. */
export function searchLanguages(query: string, ui: string): SummaryLanguage[] {
  const q = plain(query);
  if (!q) return [...SUMMARY_LANGUAGES];
  return SUMMARY_LANGUAGES.filter((l) => {
    const { name, native } = languageNames(l.code, ui);
    return plain(name).includes(q) || plain(native).includes(q);
  });
}

/** The language a summary says it was written in (as a code from the list when it is one); undefined for summaries
 * from before Talaan recorded it. */
export function writtenLanguage(summary: { language?: string } | undefined): string | undefined {
  const code = summary?.language;
  return code ? (normalizeLanguage(code) ?? code) : undefined;
}

/**
 * The language to mark as current in the pickers. A summary knows its own; one from before that was recorded doesn't,
 * so nothing is marked. With no summary yet it is the language one would be written in: the one picked on the
 * meeting, else the Settings language ("meeting" follows the meeting's own).
 */
export function currentSummaryLanguage(
  meeting: { summary?: { language?: string }; summaryLanguage?: string; language: string },
  settingsLanguage: string | undefined,
): string | undefined {
  if (meeting.summary) return writtenLanguage(meeting.summary);
  const settings = settingsLanguage && settingsLanguage !== "meeting" ? settingsLanguage : undefined;
  return normalizeLanguage(meeting.summaryLanguage ?? settings ?? meeting.language);
}

/**
 * What Settings › Summary AI › Summary language shows for the saved value: the button's text, the row to mark as
 * current, and the rows above the list ("Same as meeting", and a saved language that isn't on the list, such as one
 * from an older version, so it stays selectable). "tl" and other older spellings map onto the list.
 */
export function settingLanguageView(
  value: string | undefined,
  ui: string,
  sameAsMeeting: string,
): { label: string; current: string; extra: { code: string; label: string }[] } {
  const meeting = { code: "meeting", label: sameAsMeeting };
  if (!value || value === "meeting") return { label: sameAsMeeting, current: "meeting", extra: [meeting] };
  const listed = normalizeLanguage(value);
  if (listed) return { label: languageLabel(listed, ui), current: listed, extra: [meeting] };
  const label = languageLabel(value, ui);
  return { label, current: value, extra: [meeting, { code: value, label }] };
}

/**
 * Whether the local AI model `modelId` did poorly on this language, so the list says "Works best with cloud AI".
 * Results per local model id, from the language check (docs/summary-languages.md): one synthetic meeting
 * summarized once per language with the real prompt; the languages graded weak or bad. A model with no entry, or an
 * empty one, has not been measured: qwen3.5-9b is still to do (not downloaded yet), so it shows no note.
 */
const POOR_ON_LOCAL: Readonly<Record<string, readonly string[]>> = {
  "qwen3.5-4b": ["fil", "sv", "da", "no", "fi", "pl", "ro", "el", "tr", "ar", "he", "hi", "bn", "ur", "ms", "vi", "ko"],
  "qwen3.5-9b": [],
  "granite-4.2-3b": ["fil", "es", "fr", "de", "pt", "it", "nl", "sv", "da", "no", "fi", "pl", "cs", "ro", "el", "ru", "uk", "tr", "ar", "he", "hi", "bn", "ur", "id", "ms", "vi", "th", "ja", "ko", "zh-Hant"],
};

export function cloudBetter(code: string, modelId: string | undefined): boolean {
  return modelId !== undefined && (POOR_ON_LOCAL[modelId] ?? []).includes(code);
}
