import { describe, expect, it } from "vitest";
import { seedMeetings } from "../data/mock/seed";
import { safeFileName, toEmail, toMarkdown, toPlainText, toSlack } from "./share";

const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;

describe("toSlack", () => {
  const text = toSlack(q4);
  it("uses Slack bold and bullets", () => {
    expect(text).toContain("*Q4 roadmap sync");
    expect(text).toContain("*Decisions*");
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
    expect(toSlack(noSummary)).toContain("no summary yet");
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
