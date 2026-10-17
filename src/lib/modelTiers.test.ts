import { describe, expect, it } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import { modelNote, splitByTier, tierOf, TIERS } from "./modelTiers";

describe("speech model tiers", () => {
  it("puts the three plain-language choices first and keeps the rest in catalog order", () => {
    const { tiered, more } = splitByTier(defaultSettings.speechModels);
    expect(tiered.map((m) => m.id)).toEqual(["whisper-turbo", "parakeet-v3", "whisper-small"]);
    expect(more.map((m) => m.id)).toEqual(defaultSettings.speechModels.filter((m) => !tierOf(m.id)).map((m) => m.id));
    expect(tiered.length + more.length).toBe(defaultSettings.speechModels.length);
  });

  it("names a model in every tier that exists in the catalog, the default first", () => {
    for (const x of TIERS) expect(defaultSettings.speechModels.some((m) => m.id === x.id), x.id).toBe(true);
    expect(TIERS[0].id).toBe(defaultSettings.activeSpeechModel);
  });

  it("reads the note in the app language, with the Windows wording where there is one", () => {
    const parakeet = defaultSettings.speechModels.find((m) => m.id === "parakeet-v3")!;
    expect(modelNote("en", parakeet, false)).toBe("Good for quick notes in 25 European languages.");
    expect(modelNote("en", parakeet, true)).toContain("without a graphics card");
    expect(modelNote("fr", parakeet, true)).toContain("sans carte graphique");
    const turbo = defaultSettings.speechModels.find((m) => m.id === "whisper-turbo")!;
    expect(modelNote("en", turbo, true)).toBe(modelNote("en", turbo, false));
    expect(modelNote("en", { id: "unknown", note: "As stored" }, false)).toBe("As stored");
  });
});
