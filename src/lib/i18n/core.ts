/** Talaan's own tiny i18n: typed message areas, {name} placeholders, one/other plurals, no library. */
export type Lang = "en" | "fr" | "es";
export type UiLanguage = "auto" | Lang;

export const LANGS: Lang[] = ["en", "fr", "es"];

/** Names shown in the language picker, always in their own language. */
export const LANG_NAMES: Record<Lang, string> = { en: "English", fr: "Français", es: "Español" };

/** Locale tags for dates and times. French is Canadian, Spanish is the US/Canada variant. */
export const LOCALE_TAG: Record<Lang, string> = { en: "en-US", fr: "fr-CA", es: "es-US" };

/** "auto" follows the system language when Talaan has it, and falls back to English. */
export function resolveLang(pref: UiLanguage | undefined, systemLanguages: readonly string[]): Lang {
  if (pref && pref !== "auto") return LANGS.includes(pref) ? pref : "en";
  for (const tag of systemLanguages) {
    const base = tag.toLowerCase().split("-")[0] as Lang;
    if (LANGS.includes(base)) return base;
  }
  return "en";
}

/** One area of the UI (a screen or a family of screens): the same keys in every language, enforced by the compiler. */
export interface Area<K extends string> {
  en: Record<K, string>;
  fr: Record<K, string>;
  es: Record<K, string>;
}
export function area<K extends string>(a: Area<K>): Area<K> {
  return a;
}

export type Vars = Record<string, string | number>;

export function format(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole));
}

/** Looks `key` up in `table`, preferring the plural form chosen by `vars.count` when there is one. */
export function lookup(table: Record<string, string>, key: string, lang: Lang, vars?: Vars): string | undefined {
  if (vars && typeof vars.count === "number") {
    const rule = new Intl.PluralRules(LOCALE_TAG[lang]).select(vars.count);
    const plural = table[`${key}_${rule === "one" ? "one" : "other"}`];
    if (plural !== undefined) return plural;
  }
  return table[key];
}
