import { inTauri } from "./windows";
import { onWindows } from "./os";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export function resolveTheme(pref: ThemePreference, systemPrefersDark: boolean): ResolvedTheme {
  if (pref === "system") return systemPrefersDark ? "dark" : "light";
  return pref;
}

export function applyTheme(pref: ThemePreference): () => void {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    const resolved = resolveTheme(pref, media.matches);
    document.documentElement.dataset.theme = resolved;
    // The Windows title bar is drawn by the OS: make it follow the app's theme. "system" passes null so the
    // window (and WebView2's prefers-color-scheme) keeps following the OS. (macOS has an overlay title bar.)
    if (onWindows && inTauri()) {
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(pref === "system" ? null : resolved))
        .catch(() => {});
    }
  };
  apply();
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}
