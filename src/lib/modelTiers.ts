import type { SpeechModel } from "../data/settings";
import { messageOr, type Lang, type MessageKey } from "./i18n";
import { onWindows } from "./platform";

/** The three speech models most people choose between, named for what they're good at instead of their
 *  technical names. The same three on both systems:
 *  - Best for most people: Whisper Large Turbo, the most accurate and the only one of the three for Filipino and
 *    the other non-European languages. It is the default, and it uses the graphics chip on a Mac and the graphics
 *    card on Windows when there is one.
 *  - Fastest: Parakeet (v3), many times faster when speech runs on the processor (a Windows PC without a graphics
 *    card), in 25 European languages.
 *  - For older computers: Whisper Small, the smallest download that still handles every language reasonably.
 *  Every other model is listed under "More models". */
export const TIERS = [
  { id: "whisper-turbo", label: "settings.tier_best" },
  { id: "parakeet-v3", label: "settings.tier_fast" },
  { id: "whisper-small", label: "settings.tier_old" },
] as const satisfies readonly { id: string; label: MessageKey }[];

export const tierOf = (id: string) => TIERS.find((x) => x.id === id);

/** The tiered models first, in tier order, then the rest as the catalog lists them. */
export function splitByTier<M extends { id: string }>(models: M[]): { tiered: M[]; more: M[] } {
  const tiered = TIERS.map((x) => models.find((m) => m.id === x.id)).filter((m): m is M => !!m);
  return { tiered, more: models.filter((m) => !tierOf(m.id)) };
}

/** A model's one-line note in the app language. Windows says when a model is the quick one on the processor. */
export function modelNote(lang: Lang, m: Pick<SpeechModel, "id" | "note">, windows = onWindows): string {
  const own = messageOr(lang, `settings.model_note_${m.id}`, m.note);
  return windows ? messageOr(lang, `settings.model_note_${m.id}_windows`, own) : own;
}
