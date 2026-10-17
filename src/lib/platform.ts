/**
 * The few words and flows that differ between Mac and Windows. Copy is platform-neutral everywhere else;
 * screens read these instead of checking the OS themselves. The words live in lib/i18n/messages/platform.ts.
 */
import { useLang } from "./i18n/I18nProvider";
import { messageOr, type Lang } from "./i18n";

export type OS = "mac" | "windows";

/** WebView2 reports "Windows NT"; WKWebView (and the browser preview on a Mac) report "Macintosh". */
export function detectOS(userAgent: string): OS {
  return /Windows/i.test(userAgent) ? "windows" : "mac";
}

export interface PlatformCopy {
  /** Button that opens the OS settings app. */
  openSettings: string;
  /** The OS settings app's name, mid-sentence. */
  settingsApp: string;
  /** Where the user turns the microphone on for Talaan. */
  micSettingsPath: string;
  /** macOS asks for system audio (process tap); Windows loopback needs no permission. */
  systemAudioNeedsPermission: boolean;
  /** Where Talaan's icon lives. */
  trayName: string;
  /** Onboarding "Allow two kinds of audio": the intro and the System audio card. */
  audioIntro: string;
  systemAudioDesc: string;
  /** Mac: the permission's name as the system lists it, shown in bold where `systemAudioDesc` says {permission}. */
  systemAudioPermission?: string;
}

export function platformCopy(os: OS, lang: Lang = "en"): PlatformCopy {
  const w = (k: string) => messageOr(lang, `platform.${os}_${k}`, k);
  return {
    openSettings: w("openSettings"),
    settingsApp: w("settingsApp"),
    micSettingsPath: w("micSettingsPath"),
    systemAudioNeedsPermission: os === "mac",
    trayName: w("trayName"),
    audioIntro: w("audioIntro"),
    systemAudioDesc: w("systemAudioDesc"),
    systemAudioPermission: os === "mac" ? w("systemAudioPermission") : undefined,
  };
}

/** The browser preview (no Tauri) can show the other platform's words: `?os=mac` or `?os=windows` in the URL. */
function previewOverride(): OS | undefined {
  if (typeof window === "undefined" || "__TAURI_INTERNALS__" in window) return undefined;
  const v = new URLSearchParams(window.location.search).get("os");
  return v === "mac" || v === "windows" ? v : undefined;
}

export const os: OS = previewOverride() ?? detectOS(typeof navigator === "undefined" ? "" : navigator.userAgent);

/** For the few flows (not words) that differ: the Windows calendar address, Ctrl vs Cmd, the macOS title bar inset. */
export const onWindows = os === "windows";
export const onMac = os === "mac";

/** The platform words in the current UI language. Re-renders when the language changes. */
export function usePlatform(): PlatformCopy {
  return platformCopy(os, useLang());
}

/** The flags that don't depend on language (onboarding's first state). */
export const systemAudioNeedsPermission = os === "mac";
