import { describe, expect, it } from "vitest";
import { translate, type Lang } from "./i18n";
import { deletedMessage } from "./modelDelete";

const say = (lang: Lang, r: { erased: boolean; otherApps: string[] }) => deletedMessage((k, v) => translate(lang, k, v), lang, "Whisper Small", r);

describe("the message after Delete on a model", () => {
  it("says nothing when the file was erased", () => {
    expect(say("en", { erased: true, otherApps: [] })).toBeUndefined();
  });

  it("says which apps keep the file on this computer", () => {
    expect(say("en", { erased: false, otherApps: ["Ensayo"] })).toBe("Talaan no longer uses Whisper Small. Ensayo still uses it, so it stays on this computer.");
    expect(say("en", { erased: false, otherApps: ["Ensayo", "Parlato"] })).toBe("Talaan no longer uses Whisper Small. Ensayo and Parlato still use it, so it stays on this computer.");
    expect(say("fr", { erased: false, otherApps: ["Ensayo", "Parlato"] })).toContain("Ensayo et Parlato");
    expect(say("es", { erased: false, otherApps: ["Ensayo", "Parlato"] })).toContain("Ensayo y Parlato");
  });
});
