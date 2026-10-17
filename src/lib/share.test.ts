import { describe, expect, it } from "vitest";
import { seedMeetings } from "../data/mock/seed";
import { safeFileName, toEmail, toMarkdown, toPlainText, toShareText } from "./share";

const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;

describe("toShareText", () => {
  const text = toShareText(q4);
  it("uses plain headings and bullets, no markup", () => {
    expect(text.startsWith("Q4 roadmap sync")).toBe(true);
    expect(text).toContain("\nDecisions\n");
    expect(text).not.toContain("*");
    expect(text).toContain("• Onboarding revamp ships first; pricing moves to Q1. (00:21)");
  });
  it("lists action items with owner and due date", () => {
    expect(text).toContain("• ☐ Send onboarding wireframes to design — Me, due Fri Oct 17");
    expect(text).toContain("• ☐ Find an owner for onboarding analytics — Priya Shah");
  });
});

describe("toEmail", () => {
  it("starts with a subject line and has no markup", () => {
    const text = toEmail(q4);
    expect(text.startsWith("Subject: Q4 roadmap sync")).toBe(true);
    expect(text).not.toContain("*");
  });
});

describe("toMarkdown", () => {
  it("includes the transcript only when asked", () => {
    expect(toMarkdown(q4, false)).not.toContain("## Transcript");
    const full = toMarkdown(q4, true);
    expect(full).toContain("## Transcript");
    expect(full).toContain("**00:06 Me:** Sounds good.");
  });
  it("uses checkboxes for action items", () => {
    expect(toMarkdown(q4, false)).toContain("- [ ] Confirm the two-sprint estimate — Dana Reyes, due Thu Oct 16");
  });
  it("says so when there is no summary", () => {
    const noSummary = { ...q4, summary: undefined };
    expect(toMarkdown(noSummary, false)).toContain("_No summary yet._");
    expect(toShareText(noSummary)).toContain("no summary yet");
  });
});

describe("notes, sections and highlights", () => {
  const oneOnOne = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-1on1")!;
  it("lists your notes after the recap, with their time or 'after the call'", () => {
    const md = toMarkdown(q4, false);
    expect(md).toContain("### Your notes\n\n- onboarding first, pricing waits for Q1 (00:22)\n- check with the data team (01:12)\n- Ask Priya if analytics needs budget (after the call)");
    expect(md.indexOf("### Your notes")).toBeLessThan(md.indexOf("### Decisions"));
    // Notes are kept even before there is a summary.
    expect(toMarkdown({ ...q4, summary: undefined }, false)).toContain("### Your notes");
  });
  it("adds the meeting type's sections and the highlights", () => {
    expect(toMarkdown(oneOnOne, false)).toContain("### Support needed\n\n- Jordan needs a data analyst before January. (20:40)");
    expect(toMarkdown(q4, false)).toContain("### Highlights\n\n- You promised design the onboarding wireframes by Friday");
    expect(toShareText(oneOnOne)).toContain("\nFeedback\n• Jordan liked");
  });
  it("leaves a meeting without notes or extras as it was", () => {
    const plain = { ...q4, marks: [], summary: { ...q4.summary!, highlights: undefined } };
    const md = toMarkdown(plain, false);
    expect(md).not.toContain("Your notes");
    expect(md).not.toContain("Highlights");
  });
});

describe("toPlainText", () => {
  it("strips the Markdown markup", () => {
    const text = toPlainText(q4, true);
    expect(text).not.toMatch(/^#/m);
    expect(text).not.toContain("**");
    expect(text).not.toContain("[ ]");
  });
});

describe("safeFileName", () => {
  it("removes characters that break file names", () => {
    expect(safeFileName('Vendor demo: "email" / platform')).toBe("Vendor demo email  platform");
    expect(safeFileName("///")).toBe("Meeting");
  });
});
