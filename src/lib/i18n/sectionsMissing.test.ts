import { describe, expect, it } from "vitest";
import { templateName } from "../templates";
import { translate } from "./index";

// The summary card's (SummaryCard.tsx) "sections couldn't be written" notice puts a meeting type's name (a capitalized label such
// as "Reunión individual") mid-sentence. French and Spanish quote it so it reads as a name, not a stray capital.
const notice = (lang: "en" | "fr" | "es") => translate(lang, "meeting.sectionsMissing", { type: translate(lang, templateName("one_on_one")) });

describe("meeting.sectionsMissing", () => {
  it("quotes the meeting type in Spanish and French", () => {
    expect(notice("es")).toBe("Esta vez no se pudieron escribir las secciones de “Reunión individual”.");
    expect(notice("fr")).toBe("Les sections « Rencontre individuelle » n’ont pas pu être rédigées cette fois.");
  });
  it("keeps English as it is", () => {
    expect(notice("en")).toBe("The 1:1 sections couldn't be written this time.");
  });
});
