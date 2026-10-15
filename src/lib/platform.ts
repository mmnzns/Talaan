/**
 * The few words and flows that differ between Mac and Windows. Copy is platform-neutral everywhere else;
 * screens read these instead of checking the OS themselves.
 */
export type OS = "mac" | "windows";

/** WebView2 reports "Windows NT"; WKWebView (and the browser preview on a Mac) report "Macintosh". */
export function detectOS(userAgent: string): OS {
  return /Windows/i.test(userAgent) ? "windows" : "mac";
}

export interface PlatformCopy {
  /** Button that opens the OS settings app. */
  openSettings: string;
  /** "Allow in …" link text target, mid-sentence. */
  settingsApp: string;
  /** Where the user turns the microphone on for Talaan. */
  micSettingsPath: string;
  /** macOS asks for system audio (process tap); Windows loopback needs no permission. */
  systemAudioNeedsPermission: boolean;
  /** Where Talaan's icon lives. */
  trayName: string;
}

const COPY: Record<OS, PlatformCopy> = {
  mac: {
    openSettings: "Open System Settings",
    settingsApp: "System Settings",
    micSettingsPath: "System Settings › Privacy & Security › Microphone",
    systemAudioNeedsPermission: true,
    trayName: "menu bar",
  },
  windows: {
    openSettings: "Open Windows Settings",
    settingsApp: "Windows Settings",
    micSettingsPath: "Windows Settings › Privacy & security › Microphone",
    systemAudioNeedsPermission: false,
    trayName: "system tray",
  },
};

export function platformCopy(os: OS): PlatformCopy {
  return COPY[os];
}

export const os: OS = detectOS(typeof navigator === "undefined" ? "" : navigator.userAgent);
export const platform: PlatformCopy = platformCopy(os);
