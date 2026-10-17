import { describe, expect, it } from "vitest";
import { newIssueUrl, setupLines } from "./diagnostics";

describe("diagnostics", () => {
  it("writes one line per check", () => {
    expect(setupLines([{ id: "mic", title: "Microphone access", status: "ok", detail: "Allowed." }])).toEqual(["ok · Microphone access · Allowed."]);
    expect(setupLines(undefined)).toEqual([]);
  });

  it("opens a new issue with only the version filled in", () => {
    const url = new URL(newIssueUrl("1.1.0"));
    expect(url.origin + url.pathname).toBe("https://github.com/mmnzns/Talaan/issues/new");
    expect([...url.searchParams.keys()]).toEqual(["body"]);
    expect(url.searchParams.get("body")).toContain("Talaan version: 1.1.0");
  });
});
