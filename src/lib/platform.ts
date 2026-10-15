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
  /** Onboarding "Allow two kinds of audio": the intro and the System audio card. */
  audioIntro: string;
  systemAudioDesc: string;
}

const COPY: Record<OS, PlatformCopy> = {
  mac: {
    openSettings: "Open System Settings",
    settingsApp: "System Settings",
    micSettingsPath: "System Settings › Privacy & Security › Microphone",
    systemAudioNeedsPermission: true,
    trayName: "menu bar",
    audioIntro: "Talaan needs your microphone (your side) and system audio (everyone else). Your system will ask for each one next.",
    systemAudioDesc: "Records the other people on the call. Your system may list this as screen and system audio recording; Talaan only uses the audio.",
  },
  windows: {
    openSettings: "Open Windows Settings",
    settingsApp: "Windows Settings",
    micSettingsPath: "Windows Settings › Privacy & security › Microphone",
    systemAudioNeedsPermission: false,
    trayName: "system tray",
    audioIntro: "Talaan needs your microphone (your side) and system audio (everyone else). System audio needs no permission here.",
    systemAudioDesc: "Records the other people on the call. No permission needed.",
  },
};

export function platformCopy(os: OS): PlatformCopy {
  return COPY[os];
}

export const os: OS = detectOS(typeof navigator === "undefined" ? "" : navigator.userAgent);
export const platform: PlatformCopy = platformCopy(os);
