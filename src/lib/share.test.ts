import { describe, expect, it } from "vitest";
import { seedMeetings } from "../data/mock/seed";
import { MAILTO_LIMIT, safeFileName, toEmail, toEmailBody, toMailto, toMarkdown, toPlainText, toShareText } from "./share";

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

describe("toMailto", () => {
  const withEmails = { ...q4, calendarEvent: { title: "Q4 Roadmap Sync", attendees: ["Dana Reyes", "Priya Shah"], emails: ["dana@example.com", "priya.shah@example.com", "not an address"] } };

  it("sends to the attendees with the title as the subject", () => {
    const { url, truncated } = toMailto(withEmails);
    expect(truncated).toBe(false);
    expect(url.startsWith("mailto:dana@example.com,priya.shah@example.com?subject=Q4%20roadmap%20sync&body=")).toBe(true);
    const body = decodeURIComponent(url.split("&body=")[1]);
    expect(body).toBe(toEmailBody(q4).replace(/\n/g, "\r\n"));
    expect(body).toContain("Onboarding revamp ships first");
    expect(body).toContain("Send onboarding wireframes to design");
    expect(body).not.toContain("Open questions");
  });

  it("has no recipients when the calendar had no addresses", () => {
    expect(toMailto({ ...q4, calendarEvent: undefined }).url.startsWith("mailto:?subject=")).toBe(true);
  });

  it("shortens a long body to fit and says the full notes come separately", () => {
    const long = { ...withEmails, summary: { ...q4.summary!, recap: "Long recap with an emoji 🙂 here. ".repeat(120) } };
    const { url, truncated } = toMailto(long);
    expect(truncated).toBe(true);
    expect(url.length).toBeLessThanOrEqual(MAILTO_LIMIT);
    expect(url.length).toBeGreaterThan(MAILTO_LIMIT - 200);
    expect(decodeURIComponent(url.split("&body=")[1])).toMatch(/…\r\n\r\nFull notes attached separately\.$/);
  });
});
