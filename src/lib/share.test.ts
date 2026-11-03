import { describe, expect, it } from "vitest";
import { seedMeetings } from "../data/mock/seed";
import { MAILTO_LIMIT, safeFileName, toEmail, toEmailBody, toMailto, toCopyText, toMarkdown, toPlainText, toShareText } from "./share";

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
    expect(toMarkdown(q4, "summary")).not.toContain("## Transcript");
    const full = toMarkdown(q4, "both");
    expect(full).toContain("## Transcript");
    expect(full).toContain("**00:06 Me:** Sounds good.");
  });
  it("uses checkboxes for action items", () => {
    expect(toMarkdown(q4, "summary")).toContain("- [ ] Confirm the two-sprint estimate — Dana Reyes, due Thu Oct 16");
  });
  it("says so when there is no summary", () => {
    const noSummary = { ...q4, summary: undefined };
    expect(toMarkdown(noSummary, "summary")).toContain("_No summary yet._");
    expect(toShareText(noSummary)).toContain("no summary yet");
  });
});

describe("notes, sections and highlights", () => {
  const oneOnOne = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-1on1")!;
  it("lists your notes after the recap, with their time or 'after the call'", () => {
    const md = toMarkdown(q4, "summary");
    expect(md).toContain("### Your notes\n\n- onboarding first, pricing waits for Q1 (00:22)\n- check with the data team (01:12)\n- Ask Priya if analytics needs budget (after the call)");
    expect(md.indexOf("### Your notes")).toBeLessThan(md.indexOf("### Decisions"));
    // Notes are kept even before there is a summary.
    expect(toMarkdown({ ...q4, summary: undefined }, "summary")).toContain("### Your notes");
  });
  it("adds the meeting type's sections and the highlights", () => {
    expect(toMarkdown(oneOnOne, "summary")).toContain("### Support needed\n\n- Jordan needs a data analyst before January. (20:40)");
    expect(toMarkdown(q4, "summary")).toContain("### Highlights\n\n- You promised design the onboarding wireframes by Friday");
    expect(toShareText(oneOnOne)).toContain("\nFeedback\n• Jordan liked");
    expect(toShareText(q4)).toContain("\nYour notes\n• onboarding first, pricing waits for Q1 (00:22)");
    // Before there is a summary, the notes still come along.
    const noSummary = toShareText({ ...q4, summary: undefined });
    expect(noSummary).toContain("(no summary yet)\n\nYour notes\n• onboarding first");
    expect(toShareText({ ...q4, summary: undefined, marks: [] })).not.toContain("Your notes");
  });
  it("leaves a meeting without notes or extras as it was", () => {
    const plain = { ...q4, marks: [], summary: { ...q4.summary!, highlights: undefined } };
    const md = toMarkdown(plain, "summary");
    expect(md).not.toContain("Your notes");
    expect(md).not.toContain("Highlights");
  });
});

describe("toPlainText", () => {
  it("strips the Markdown markup", () => {
    const text = toPlainText(q4, "both");
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

  it("shortens a long body to fit and says where the full notes are", () => {
    const long = { ...withEmails, summary: { ...q4.summary!, recap: "Long recap with an emoji 🙂 here. ".repeat(120) } };
    const { url, truncated } = toMailto(long);
    expect(truncated).toBe(true);
    expect(url.length).toBeLessThanOrEqual(MAILTO_LIMIT);
    expect(url.length).toBeGreaterThan(MAILTO_LIMIT - 200);
    expect(decodeURIComponent(url.split("&body=")[1])).toMatch(/…\r\n\r\nThe full notes are in Talaan.$/);
  });
});

describe("export scope", () => {
  const first = q4.transcript[0];
  const line = (l: typeof first) => l.text;

  it("summary only has no transcript, transcript only has no summary, both has each", () => {
    const summary = toMarkdown(q4, "summary");
    const transcript = toMarkdown(q4, "transcript");
    const both = toMarkdown(q4, "both");
    expect(summary).toContain("## Summary");
    expect(summary).not.toContain(line(first));
    expect(transcript).toContain("## Transcript");
    expect(transcript).toContain(line(first));
    expect(transcript).not.toContain("## Summary");
    expect(transcript).not.toContain("### Decisions");
    expect(transcript.startsWith("# Q4 roadmap sync")).toBe(true);
    expect(both).toContain("## Summary");
    expect(both).toContain(line(first));
  });
  it("says so when a transcript-only export has no transcript", () => {
    expect(toMarkdown({ ...q4, transcript: [] }, "transcript")).toContain("_No transcript yet._");
  });
  it("plain text follows the same scope without markup", () => {
    const text = toPlainText(q4, "transcript");
    expect(text).toContain(line(first));
    expect(text).not.toContain("Decisions");
    expect(text).not.toContain("**");
  });
  it("copy: summary is the share text, the other choices carry the transcript", () => {
    expect(toCopyText(q4, "summary")).toBe(toShareText(q4));
    expect(toCopyText(q4, "both")).toBe(toPlainText(q4, "both"));
    const only = toCopyText(q4, "transcript");
    expect(only).toContain(line(first));
    expect(only).not.toContain("Decisions");
  });
});
