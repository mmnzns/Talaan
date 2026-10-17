import { describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import type { Settings } from "../data/settings";
import { privacyChip } from "./privacyChip";

const s = (patch: Partial<Settings> = {}): Settings => ({ ...structuredClone(defaultSettings), ...patch });

describe("privacyChip", () => {
  it("says everything stays here only with the built-in AI and nothing online", () => {
    expect(privacyChip(s(), true)).toBe("shell.offlineLocal");
    expect(privacyChip(s({ calendarConnected: true }), false)).toBe("shell.offlineLocal"); // the Mac reads its own calendar
  });
  it("is softer when Notion or the Windows calendar address is set up", () => {
    expect(privacyChip(s({ notion: { connected: true } }), false)).toBe("shell.summariesLocal");
    expect(privacyChip(s({ calendarConnected: true }), true)).toBe("shell.summariesLocal");
  });
  it("names an online AI account, and a server that isn't on this computer", () => {
    expect(privacyChip(s({ summaryEngine: "byok" }), true)).toBe("shell.cloudOn");
    expect(privacyChip(s({ summaryEngine: "ollama", ollamaAddress: "http://localhost:11434" }), true)).toBe("shell.summariesLocal");
    expect(privacyChip(s({ summaryEngine: "ollama", ollamaAddress: "127.0.0.1:1234" }), true)).toBe("shell.summariesLocal");
    expect(privacyChip(s({ summaryEngine: "ollama", ollamaAddress: "http://gpu-box.lan:11434" }), true)).toBe("shell.summariesServer");
  });
  it("claims nothing before settings load", () => {
    expect(privacyChip(undefined)).toBe("shell.summariesLocal");
  });
});
