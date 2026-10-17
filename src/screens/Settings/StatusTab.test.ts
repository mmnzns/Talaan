import { describe, expect, it } from "vitest";
import { LANGS, translate } from "../../lib/i18n";
import { gpuTitle } from "./StatusTab";

describe("the graphics switch's name", () => {
  it("says graphics card on Windows and graphics chip on a Mac, in every language", () => {
    expect(translate("en", gpuTitle(true))).toBe("Speed up with your graphics card");
    expect(translate("en", gpuTitle(false))).toBe("Speed up with the graphics chip");
    for (const lang of LANGS) {
      expect(translate(lang, gpuTitle(false))).not.toMatch(/card|carte|tarjeta/i);
      expect(translate(lang, gpuTitle(true))).toMatch(/card|carte|tarjeta/i);
    }
  });
});
