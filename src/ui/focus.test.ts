import { describe, expect, it } from "vitest";
import { trapTarget } from "./focus";

describe("trapTarget", () => {
  it("lets the browser move focus inside the dialog", () => {
    expect(trapTarget(3, 0, false)).toBeNull();
    expect(trapTarget(3, 1, false)).toBeNull();
    expect(trapTarget(3, 2, true)).toBeNull();
    expect(trapTarget(3, 1, true)).toBeNull();
  });
  it("wraps at both ends", () => {
    expect(trapTarget(3, 2, false)).toBe(0);
    expect(trapTarget(3, 0, true)).toBe(2);
  });
  it("brings focus in from the dialog itself or from outside", () => {
    expect(trapTarget(3, -1, false)).toBe(0);
    expect(trapTarget(3, -1, true)).toBe(2);
  });
  it("a single control keeps focus on itself", () => {
    expect(trapTarget(1, 0, false)).toBe(0);
    expect(trapTarget(1, 0, true)).toBe(0);
  });
  it("does nothing without controls", () => {
    expect(trapTarget(0, -1, false)).toBeNull();
  });
});
