import { routes, navigate } from "./router";

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Opens the Settings window (a separate Tauri window). In a plain browser it navigates instead. */
export async function openSettings(tab = "general"): Promise<void> {
  if (!inTauri()) {
    navigate(routes.settings(tab));
    return;
  }
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const existing = await WebviewWindow.getByLabel("settings");
  if (existing) {
    await existing.setFocus();
    return;
  }
  new WebviewWindow("settings", {
    url: `index.html${routes.settings(tab)}`,
    title: "Settings",
    width: 780,
    height: 640,
    minWidth: 680,
    minHeight: 520,
    center: true,
  });
}
