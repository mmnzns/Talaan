import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { call } from "../../data/tauri/invoke";
import { inTauri } from "../windows";
import { useSettings } from "../../data/hooks";
import { languageFor, setLang, translate, type Lang, type T } from "./index";

interface Ctx {
  lang: Lang;
  t: T;
}

const I18nContext = createContext<Ctx>({ lang: "en", t: (key, vars) => translate("en", key, vars) });

/** Follows `settings.uiLanguage` (live, across windows) and keeps the module-level `t` in step for non-React code. */
export function I18nProvider({ children }: { children: ReactNode }) {
  const { settings } = useSettings();
  const lang = languageFor(settings?.uiLanguage);
  setLang(lang); // during render, so children and non-React callers agree within one pass
  const value = useMemo<Ctx>(() => ({ lang, t: (key, vars) => translate(lang, key, vars) }), [lang]);
  // The menu bar menu, notifications and recorder warnings are made in Rust; it needs the resolved language too.
  useEffect(() => {
    if (inTauri()) call("i18n_set_language", { lang }).catch(() => {});
  }, [lang]);
  if (typeof document !== "undefined") document.documentElement.lang = lang; // screen readers pick the voice from this
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export const useT = (): T => useContext(I18nContext).t;
export const useLang = (): Lang => useContext(I18nContext).lang;
