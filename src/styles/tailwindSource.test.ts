import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// Tailwind scans the whole repository unless told otherwise. docs/, site/ and the changelog are prose with Windows
// paths and stray words ("inline", "ring", "[mm:ss]") that turn into utility classes the app never uses.
describe("index.css", () => {
  it("limits the Tailwind scan to the frontend source", () => {
    const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");
    expect(css).toMatch(/@import "tailwindcss" source\("[^"]+"\);/);
  });
});
