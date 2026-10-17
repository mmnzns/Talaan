import { describe, expect, it } from "vitest";
import { mailtoHead, mailtoLink, mailtoRecipients } from "./mailto";

describe("mailto", () => {
  it("keeps a literal @, escapes around it, drops bad and repeated addresses", () => {
    expect(mailtoRecipients([" ana@example.com ", "o'neil+team@example.com", "not an address", "ANA@example.com", "a@b,c@d"])).toEqual([
      "ana@example.com",
      "o'neil%2Bteam@example.com",
    ]);
  });

  it("builds the same link for both emails", () => {
    expect(mailtoHead([], "Q4 sync")).toBe("mailto:?subject=Q4%20sync&body=");
    expect(mailtoLink(["ana@example.com"], "Hi", "a\nb")).toBe("mailto:ana@example.com?subject=Hi&body=a%0D%0Ab");
  });
});
