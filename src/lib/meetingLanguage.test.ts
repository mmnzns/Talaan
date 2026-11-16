import { describe, expect, it } from "vitest";
import { COHERE_LANGUAGES } from "../data/settings";
import { cannotDetectLanguage, effectiveMeetingLanguage, languagesFor } from "./meetingLanguage";

const cohere = { detectsLanguage: false, languageCodes: COHERE_LANGUAGES };
const whisper = {};

describe("meeting language for a model that can't detect it", () => {
  it("only Cohere-like models can't detect", () => {
    expect(cannotDetectLanguage(cohere)).toBe(true);
    expect(cannotDetectLanguage(whisper)).toBe(false);
    expect(cannotDetectLanguage({ detectsLanguage: true })).toBe(false);
    expect(cannotDetectLanguage(undefined)).toBe(false);
  });

  it("Auto means English for it, and a picked language stays", () => {
    expect(effectiveMeetingLanguage(cohere, "auto")).toBe("en");
    expect(effectiveMeetingLanguage(cohere, "fr")).toBe("fr");
    expect(effectiveMeetingLanguage(whisper, "auto")).toBe("auto");
  });

  it("offers only its 14 languages, and keeps an old pick it can't write visible", () => {
    expect(languagesFor(whisper, "auto")).toBeNull();
    expect(languagesFor(cohere, "auto")).toHaveLength(14);
    expect(languagesFor(cohere, "fr")).toHaveLength(14);
    expect(languagesFor(cohere, "tl")).toEqual(["tl", ...COHERE_LANGUAGES]);
  });
});
