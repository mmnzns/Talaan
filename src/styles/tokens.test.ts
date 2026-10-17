import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

// Faint text (labels, timestamps, the meeting list's second line) must stay readable: WCAG AA, 4.5:1, on every
// background it sits on, in both themes.
const block = (selector: string) => css.slice(css.indexOf(selector), css.indexOf("}", css.indexOf(selector)));
const token = (b: string, name: string) => b.match(new RegExp(String.raw`--t-${name}:\s*(#[0-9A-Fa-f]{6})`))![1];
const lum = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (a: string, b: string) => {
  const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
};

describe("--t-faint", () => {
  for (const [theme, selector] of [["light", ":root {"], ["dark", ':root[data-theme="dark"] {']] as const) {
    it(`passes AA on paper, cards, the list column and row hover (${theme})`, () => {
      const b = block(selector);
      const faint = token(b, "faint");
      for (const bg of ["paper", "surface", "raised", "rail", "hover"]) {
        expect(contrast(faint, token(b, bg)), `${theme} faint on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});
