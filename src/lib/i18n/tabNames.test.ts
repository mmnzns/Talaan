import { describe, expect, it } from "vitest";
import { SETTINGS_TABS } from "../../screens/Settings/SettingsWindow";
import { LANGS, translate } from "./index";
import { messages } from "./index";

// "Settings › Speech models" in any message must name a Settings tab as it is labeled, in that language, so
// setup and the banners send people somewhere that exists. The system's own settings ("Windows Settings ›",
// "System Settings ›") are other apps and are not checked.
const SETTINGS_WORD = { en: "Settings", fr: "Paramètres", es: "Ajustes" } as const;

describe("messages that point into Settings", () => {
  for (const lang of LANGS) {
    it(`name a real tab (${lang})`, () => {
      const tabs = SETTINGS_TABS.map((x) => translate(lang, x.label));
      const word = SETTINGS_WORD[lang];
      const re = new RegExp(`(?<!Windows |System )${word} › ([^.,;:)]+)`, "gu");
      const bad: string[] = [];
      let found = 0;
      for (const [key, text] of Object.entries(messages[lang])) {
        if (key.startsWith("platform.")) continue;
        for (const m of text.matchAll(re)) {
          found++;
          if (!tabs.some((tab) => m[1].toLowerCase().startsWith(tab.toLowerCase()))) bad.push(`${key}: ${m[0]}`);
        }
      }
      expect(bad).toEqual([]);
      expect(found).toBeGreaterThan(3);
    });
  }

  it("has Status first", () => {
    expect(SETTINGS_TABS[0].id).toBe("status");
  });
});
