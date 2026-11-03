import { describe, expect, it } from "vitest";
import { defaultSettings } from "../../data/defaultSettings";
import { LANGS, translate, type Lang } from "../../lib/i18n";
import { localOption } from "./SummaryAITab";

const optionsIn = (lang: Lang) => defaultSettings.localModels.map((m) => localOption((k, v) => translate(lang, k, v), m));

describe("the Local model list", () => {
  it("has one option per local model, with size, memory and licence", () => {
    const [qwen4, qwen9, granite] = optionsIn("en");
    expect(qwen4).toBe("Qwen3.5 4B (Recommended) · 2.7 GB · needs 3.3 GB of memory · Apache-2.0");
    expect(qwen9).toBe("Qwen3.5 9B · 5.7 GB · needs 6.8 GB of memory · Apache-2.0");
    expect(granite).toBe("Granite 4.2 3B · 2.2 GB · needs 2.7 GB of memory · Apache-2.0");
  });

  it("marks only the recommended model, in every language", () => {
    for (const lang of LANGS) {
      const word = translate(lang, "settings.local_recommended");
      const marked = optionsIn(lang).filter((o) => o.includes(word));
      expect(marked, lang).toHaveLength(1);
      expect(marked[0], lang).toContain("Qwen3.5 4B");
    }
  });
});
