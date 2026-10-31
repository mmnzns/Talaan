import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  SUMMARY_LANGUAGES,
  cloudBetter,
  currentSummaryLanguage,
  isSummaryLanguage,
  languageNames,
  normalizeLanguage,
  searchLanguages,
  settingLanguageView,
  writtenLanguage,
} from "./summaryLanguages";

const codes = (list: readonly { code: string }[]) => list.map((l) => l.code);

describe("the summary languages", () => {
  it("are the 32 codes of the shared file, in its order", () => {
    const file = JSON.parse(readFileSync(new URL("./summaryLanguages.json", import.meta.url), "utf8")) as { code: string }[];
    expect(codes(SUMMARY_LANGUAGES)).toEqual(file.map((l) => l.code));
    expect(SUMMARY_LANGUAGES).toHaveLength(32);
    expect(SUMMARY_LANGUAGES[0].code).toBe("en");
    expect(codes(SUMMARY_LANGUAGES)).toContain("fil");
    expect(codes(SUMMARY_LANGUAGES)).toEqual(expect.arrayContaining(["zh-Hans", "zh-Hant"]));
  });

  it("are read by the Rust side from the same file", () => {
    const rust = readFileSync(new URL("../../src-tauri/src/summarizer/prompt.rs", import.meta.url), "utf8");
    expect(rust).toContain("src/lib/summaryLanguages.json");
  });

  it("knows which codes are offered", () => {
    expect(isSummaryLanguage("ja")).toBe(true);
    expect(isSummaryLanguage("tl")).toBe(false);
    expect(isSummaryLanguage("meeting")).toBe(false);
    expect(isSummaryLanguage(undefined)).toBe(false);
  });
});

describe("names", () => {
  it("come in the app's language and in the language itself", () => {
    expect(languageNames("ja", "en")).toEqual({ name: "Japanese", native: "日本語" });
    expect(languageNames("ja", "fr").name).toBe("japonais");
    expect(languageNames("es", "es").name).toBe("español");
    expect(languageNames("fil", "en").name).toBe("Filipino");
  });

  it("never fail on a code the platform doesn't know", () => {
    expect(languageNames("xx-nope-1234", "en").name).toBeTruthy();
    expect(languageNames("", "en").name).toBe("");
  });
});

describe("search", () => {
  it("lists everything for an empty search", () => {
    expect(searchLanguages("", "en")).toHaveLength(32);
    expect(searchLanguages("   ", "en")).toHaveLength(32);
  });

  it("finds Japanese by 'jap' and by '日本'", () => {
    expect(codes(searchLanguages("jap", "en"))).toEqual(["ja"]);
    expect(codes(searchLanguages("日本", "en"))).toEqual(["ja"]);
  });

  it("ignores case and accents, in either name", () => {
    expect(codes(searchLanguages("ESPANOL", "en"))).toEqual(["es"]);
    expect(codes(searchLanguages("espagnol", "fr"))).toEqual(["es"]);
    expect(codes(searchLanguages("francais", "en"))).toEqual(["fr"]);
    expect(codes(searchLanguages("Türk", "en"))).toEqual(["tr"]);
  });

  it("finds Filipino and both kinds of Chinese", () => {
    expect(codes(searchLanguages("filip", "en"))).toEqual(["fil"]);
    expect(codes(searchLanguages("chin", "en"))).toEqual(expect.arrayContaining(["zh-Hans", "zh-Hant"]));
    expect(searchLanguages("klingon", "en")).toEqual([]);
  });
});

describe("normalizeLanguage", () => {
  it("maps what the speech model and older settings hold onto the list", () => {
    expect(normalizeLanguage("tl")).toBe("fil");
    expect(normalizeLanguage("zh")).toBe("zh-Hans");
    expect(normalizeLanguage("en-US")).toBe("en");
    expect(normalizeLanguage("pt-BR")).toBe("pt");
    expect(normalizeLanguage("zh-Hant")).toBe("zh-Hant");
    expect(normalizeLanguage("nb")).toBe("no");
    expect(normalizeLanguage("xx")).toBeUndefined();
    expect(normalizeLanguage(undefined)).toBeUndefined();
  });
});

describe("writtenLanguage", () => {
  it("is what the summary says, as a code from the list when it is one", () => {
    expect(writtenLanguage({ language: "fil" })).toBe("fil");
    expect(writtenLanguage({ language: "tl" })).toBe("fil");
    expect(writtenLanguage({ language: "xx" })).toBe("xx");
  });

  it("is undefined for a summary from before Talaan recorded it", () => {
    expect(writtenLanguage({})).toBeUndefined();
    expect(writtenLanguage(undefined)).toBeUndefined();
  });
});

describe("currentSummaryLanguage", () => {
  it("is the language the summary was written in, whatever was chosen since", () => {
    expect(currentSummaryLanguage({ summary: { language: "fil" }, summaryLanguage: "ja", language: "en" }, "es")).toBe("fil");
  });

  it("marks nothing for an old summary that doesn't know its language", () => {
    expect(currentSummaryLanguage({ summary: {}, summaryLanguage: "ja", language: "en" }, "es")).toBeUndefined();
  });

  it("without a summary: the one chosen on the meeting, else Settings, else the meeting's own", () => {
    expect(currentSummaryLanguage({ summaryLanguage: "ja", language: "en" }, "es")).toBe("ja");
    expect(currentSummaryLanguage({ language: "en" }, "es")).toBe("es");
    expect(currentSummaryLanguage({ language: "tl" }, "meeting")).toBe("fil");
    expect(currentSummaryLanguage({ language: "en" }, undefined)).toBe("en");
    expect(currentSummaryLanguage({ language: "xx" }, "meeting")).toBeUndefined();
  });
});

describe("cloudBetter", () => {
  it("is on for a language Qwen 3.5 4B wrote poorly, and off for English and the ones it wrote well", () => {
    expect(cloudBetter("fil", "qwen3.5-4b")).toBe(true);
    expect(cloudBetter("ur", "qwen3.5-4b")).toBe(true);
    expect(cloudBetter("en", "qwen3.5-4b")).toBe(false);
    expect(cloudBetter("ja", "qwen3.5-4b")).toBe(false);
  });

  it("is per model: a model that is not measured shows no note", () => {
    expect(cloudBetter("fil", "qwen3.5-9b")).toBe(false);
    expect(cloudBetter("fil", "granite-4.2-3b")).toBe(false);
    expect(cloudBetter("fil", "some-other-model")).toBe(false);
    expect(cloudBetter("fil", undefined)).toBe(false);
  });
});

describe("settingLanguageView (Settings › Summary AI › Summary language)", () => {
  const same = "Same as meeting";

  it("shows 'Same as meeting' as the default, with that row first", () => {
    for (const value of ["meeting", "", undefined]) {
      const v = settingLanguageView(value, "en", same);
      expect(v).toEqual({ label: same, current: "meeting", extra: [{ code: "meeting", label: same }] });
    }
  });

  it("shows a language from the list with both names and marks it", () => {
    const v = settingLanguageView("ja", "en", same);
    expect(v.label).toBe("Japanese · 日本語");
    expect(v.current).toBe("ja");
    expect(v.extra).toEqual([{ code: "meeting", label: same }]);
  });

  it("keeps an older saved code working: 'tl' is shown as Filipino and marked there", () => {
    const v = settingLanguageView("tl", "en", same);
    expect(v.label).toBe("Filipino");
    expect(v.current).toBe("fil");
    expect(v.extra).toHaveLength(1);
  });

  it("shows a saved language that isn't on the list, and offers it as a row so it stays selected", () => {
    const v = settingLanguageView("sw", "en", same);
    expect(v.label).toBe("Swahili · Kiswahili");
    expect(v.current).toBe("sw");
    expect(v.extra.map((x) => x.code)).toEqual(["meeting", "sw"]);
    expect(settingLanguageView("not a code", "en", same).label).toBe("not a code");
  });
});
