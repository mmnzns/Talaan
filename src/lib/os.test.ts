import { describe, expect, it } from "vitest";
import { onMac, onWindows } from "./os";

describe("os", () => {
  it("is a pair of booleans that are never both true", () => {
    expect(typeof onWindows).toBe("boolean");
    expect(typeof onMac).toBe("boolean");
    expect(onWindows && onMac).toBe(false);
  });
});
