import { describe, expect, it } from "vitest";
import { describeError, StoreError, summaryErrorText, toStoreError } from "./errors";

describe("toStoreError", () => {
  it("reads the { code, message } shape sent by Rust", () => {
    const e = toStoreError({ code: "not_found", message: "not found: x" });
    expect(e).toBeInstanceOf(StoreError);
    expect(e.code).toBe("not_found");
  });
  it("wraps anything else", () => {
    expect(toStoreError(new Error("boom")).code).toBe("unknown");
    expect(toStoreError("text").message).toBe("text");
  });
});

describe("summaryErrorText", () => {
  it("shows fixed reasons in the app language by code, and other failures as stored", () => {
    expect(summaryErrorText(undefined)).toBe("The transcript is saved. Write the summary whenever you’re ready.");
    expect(summaryErrorText({ code: "waiting", message: "stored English" })).toBe("This call is transcript only. Generate a summary when you want one.");
    expect(summaryErrorText({ code: "too_short", message: "stored English" })).toContain("too short");
    expect(summaryErrorText({ code: "no_engine", message: "Add your Claude API key in Settings › Summary AI." })).toContain("Settings › Summary AI");
    expect(summaryErrorText({ code: "failed", message: "Claude took too long to answer." })).toBe("Claude took too long to answer.");
  });
});

describe("describeError", () => {
  it("explains features that are not built yet", () => {
    const text = describeError({ code: "not_yet_available", message: "Writing summaries needs the summarizer (M5)" });
    expect(text).toContain("summarizer (M5)");
    expect(text).toContain("later build");
  });
  it("keeps other errors short and calm", () => {
    expect(describeError({ code: "io", message: "disk exploded" })).toBe("Something went wrong. Your data is safe.");
    expect(describeError({ code: "not_found", message: "x" })).toBe("That meeting is gone.");
  });
});
