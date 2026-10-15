import { LANGS, LOCALE_TAG, format, lookup, resolveLang, type Lang, type UiLanguage, type Vars } from "./core";
import { common } from "./messages/common";

export { LANGS, LANG_NAMES, LOCALE_TAG, resolveLang, type Lang, type UiLanguage } from "./core";

/** Every area of the UI. Add a file in `messages/`, then list it here: its keys become `area.key`. */
const areas = { common };

type Areas = typeof areas;
type KeysOf<A> = A extends { en: infer E } ? keyof E & string : never;
type Stripped<K extends string> = K extends `${infer B}_${"one" | "other"}` ? B : K;
/** "onboarding.title": a typo is a compile error. */
export type MessageKey = { [N in keyof Areas & string]: `${N}.${Stripped<KeysOf<Areas[N]>>}` }[keyof Areas & string];

const catalog: Record<Lang, Record<string, string>> = { en: {}, fr: {}, es: {} };
for (const [name, a] of Object.entries(areas)) {
  for (const lang of LANGS) {
    for (const [k, v] of Object.entries(a[lang] as Record<string, string>)) catalog[lang][`${name}.${k}`] = v;
  }
}

/** All messages, for tests. */
export const messages = catalog;

let current: Lang = "en";

export const currentLang = (): Lang => current;
export const currentLocale = (): string => LOCALE_TAG[current];
export function setLang(lang: Lang): void {
  current = lang;
}

export type T = (key: MessageKey, vars?: Vars) => string;

export function translate(lang: Lang, key: MessageKey, vars?: Vars): string {
  const found = lookup(catalog[lang], key, lang, vars) ?? lookup(catalog.en, key, "en", vars) ?? key;
  return format(found, vars);
}

/** For code outside React (store toasts, formatters). Components use `useT()` so they re-render on a language change. */
export const t: T = (key, vars) => translate(current, key, vars);

/** The language a stored preference resolves to on this system. */
export function languageFor(pref: UiLanguage | undefined): Lang {
  const system = typeof navigator === "undefined" ? [] : navigator.languages?.length ? navigator.languages : [navigator.language ?? ""];
  return resolveLang(pref, system);
}
