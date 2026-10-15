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
    // The Windows title bar is drawn by the OS: make it follow the app's theme. (macOS has an overlay title bar.)
    if (onWindows && inTauri()) {
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(resolved))
        .catch(() => {});
    }
  };
  apply();
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}
