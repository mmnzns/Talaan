import { describe, expect, it } from "vitest";
import { detectOS, platformCopy } from "./platform";

const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WIN_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

describe("detectOS", () => {
  it("reads Windows from the WebView2 user agent", () => {
    expect(detectOS(WIN_UA)).toBe("windows");
  });

  it("reads the Mac from the WKWebView user agent", () => {
    expect(detectOS(MAC_UA)).toBe("mac");
  });

  it("falls back to the Mac when the agent says nothing useful", () => {
    expect(detectOS("")).toBe("mac");
  });
});

describe("platformCopy", () => {
  it("keeps the Mac wording and permission flow", () => {
    const c = platformCopy("mac");
    expect(c.openSettings).toBe("Open System Settings");
    expect(c.micSettingsPath).toBe("System Settings › Privacy & Security › Microphone");
    expect(c.systemAudioNeedsPermission).toBe(true);
    expect(c.trayName).toBe("menu bar");
  });

  it("uses Windows names and needs no system-audio permission there", () => {
    const c = platformCopy("windows");
    expect(c.openSettings).toBe("Open Windows Settings");
    expect(c.micSettingsPath).toBe("Windows Settings › Privacy & security › Microphone");
    expect(c.systemAudioNeedsPermission).toBe(false);
    expect(c.trayName).toBe("system tray");
  });

  it("never mentions the other platform", () => {
    expect(JSON.stringify(Object.values(platformCopy("windows")))).not.toMatch(/System Settings|menu bar|Mac/);
    expect(JSON.stringify(Object.values(platformCopy("mac")))).not.toMatch(/Windows|tray/);
  });
});
