import { describe, expect, it } from "vitest";
import { followUpText, mailtoUrl, MAILTO_MAX } from "./followup";
import { isTemplate, templateName } from "./templates";

describe("mailtoUrl", () => {
  const draft = { subject: "Follow-up: Q4 sync", body: "Hi all,\n\nThanks & see you.", to: ["dana@example.com", "not an address", "priya@example.com"] };

  it("puts the invitees, subject and body in the link", () => {
    const url = mailtoUrl(draft)!;
    expect(url.startsWith("mailto:dana@example.com,priya@example.com?subject=Follow-up%3A%20Q4%20sync&body=")).toBe(true);
    expect(mailtoUrl({ ...draft, to: ["o'neil+team@example.com"] })!.startsWith("mailto:o'neil%2Bteam@example.com?")).toBe(true);
    // CRLF line breaks and an escaped ampersand, so the body survives the trip.
    expect(url).toContain("Hi%20all%2C%0D%0A%0D%0AThanks%20%26%20see%20you.");
  });

  it("works without addresses", () => {
    expect(mailtoUrl({ ...draft, to: [] })!.startsWith("mailto:?subject=")).toBe(true);
  });

  it("gives up when the link would be too long for mail apps", () => {
    expect(mailtoUrl({ ...draft, body: "x".repeat(MAILTO_MAX) })).toBeUndefined();
  });

  it("copies the subject above the body", () => {
    expect(followUpText(draft)).toBe("Follow-up: Q4 sync\n\nHi all,\n\nThanks & see you.");
    expect(followUpText({ ...draft, subject: " " })).toBe(draft.body);
  });
});

describe("meeting types", () => {
  it("names known types and reads unknown ones as General", () => {
    expect(templateName("standup")).toBe("common.template_standup");
    expect(templateName("from_the_future")).toBe("common.template_general");
    expect(isTemplate("interview") && !isTemplate(undefined)).toBe(true);
  });
});
