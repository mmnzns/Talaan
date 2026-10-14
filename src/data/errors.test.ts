import { describe, expect, it } from "vitest";
import { describeError, StoreError, toStoreError } from "./errors";

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
