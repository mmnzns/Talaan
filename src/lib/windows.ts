import { emitTo } from "@tauri-apps/api/event";
import { routes, navigate } from "./router";

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The event that asks an open Settings window to show a tab (SettingsWindow listens). */
export const SETTINGS_TAB_EVENT = "settings-tab";

/** Opens the Settings window (a separate Tauri window) on `tab`, or on Status when none is given. An open window
 *  comes to the front and, when `tab` is given, switches to it. In a plain browser it navigates instead. */
export async function openSettings(tab?: string): Promise<void> {
  if (!inTauri()) {
    navigate(routes.settings(tab));
    return;
  }
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const existing = await WebviewWindow.getByLabel("settings");
  if (existing) {
    if (tab) {
      await emitTo("settings", SETTINGS_TAB_EVENT, tab).catch(() => {});
    }
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

/** Opens a web link in the default browser (in a plain browser, a new tab). */
export async function openLink(url: string): Promise<void> {
  if (!inTauri()) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
