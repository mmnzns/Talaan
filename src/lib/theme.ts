export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export function resolveTheme(pref: ThemePreference, systemPrefersDark: boolean): ResolvedTheme {
  if (pref === "system") return systemPrefersDark ? "dark" : "light";
  return pref;
}

export function applyTheme(pref: ThemePreference): () => void {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    document.documentElement.dataset.theme = resolveTheme(pref, media.matches);
  };
  apply();
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}
