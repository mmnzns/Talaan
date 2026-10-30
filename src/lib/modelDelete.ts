import type { Lang } from "./i18n/core";
import type { T } from "./i18n";

const AND: Record<Lang, string> = { en: "and", fr: "et", es: "y" };

/** "Ensayo", "Ensayo and Parlato", "A, B and C". */
function joinNames(lang: Lang, names: string[]): string {
  if (names.length < 2) return names.join("");
  return `${names.slice(0, -1).join(", ")} ${AND[lang]} ${names[names.length - 1]}`;
}

/** What to tell the person after Delete on a model. Nothing when the file was erased (the row just changes);
 *  when another app (Ensayo, Parlato) still uses it, Talaan lets go of it and says the file stays. */
export function deletedMessage(t: T, lang: Lang, name: string, r: { erased: boolean; otherApps: string[] }): string | undefined {
  if (r.erased || r.otherApps.length === 0) return undefined;
  return t("settings.model_kept", { name, apps: joinNames(lang, r.otherApps), count: r.otherApps.length });
}
