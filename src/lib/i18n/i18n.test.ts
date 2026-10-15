import { describe, expect, it } from "vitest";
import { format, lookup, resolveLang } from "./core";
import { messages, translate } from "./index";

describe("resolveLang", () => {
  it("uses an explicit choice", () => {
    expect(resolveLang("fr", ["es-US"])).toBe("fr");
  });
  it("auto follows the first system language Talaan has", () => {
    expect(resolveLang("auto", ["de-DE", "fr-CA", "en-US"])).toBe("fr");
    expect(resolveLang("auto", ["es-MX"])).toBe("es");
  });
  it("falls back to English", () => {
    expect(resolveLang("auto", ["ja-JP"])).toBe("en");
    expect(resolveLang(undefined, [])).toBe("en");
  });
});

describe("format and lookup", () => {
  it("fills placeholders and leaves unknown ones", () => {
    expect(format("Hi {name}, {n} left, {x}", { name: "Ana", n: 3 })).toBe("Hi Ana, 3 left, {x}");
  });
  it("picks the plural form", () => {
    const table = { m_one: "{count} meeting", m_other: "{count} meetings" };
    expect(lookup(table, "m", "en", { count: 1 })).toBe("{count} meeting");
    expect(lookup(table, "m", "en", { count: 2 })).toBe("{count} meetings");
    expect(lookup({ m_one: "un", m_other: "deux" }, "m", "fr", { count: 0 })).toBe("un"); // French: 0 is singular
  });
});

describe("translate", () => {
  it("falls back to English, then to the key", () => {
    expect(translate("fr", "common.cancel")).toBe("Annuler");
    expect(translate("en", "nope.nothing" as never)).toBe("nope.nothing");
  });
});

// The rules from Parlato's French and Spanish: every sentence in all three languages, proper accents everywhere,
// the opening ¿ and ¡ in Spanish, and the same placeholders. These checks are mechanical on purpose.
describe("the catalog", () => {
  const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
  const plain = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "");

  it("has every key in every language with the same placeholders and no blanks", () => {
    for (const key of Object.keys(messages.en)) {
      for (const lang of ["fr", "es"] as const) {
        const v = messages[lang][key];
        expect(v, `${lang} ${key}`).toBeTruthy();
        expect(placeholders(v), `${lang} ${key} placeholders`).toBe(placeholders(messages.en[key]));
      }
    }
    for (const lang of ["fr", "es"] as const) {
      expect(Object.keys(messages[lang]).sort()).toEqual(Object.keys(messages.en).sort());
    }
  });

  it("spells a word the same way everywhere in a language (no unaccented stragglers)", () => {
    for (const lang of ["fr", "es"] as const) {
      const spellings = new Map<string, Set<string>>();
      for (const text of Object.values(messages[lang])) {
        for (const word of text.replace(/\{\w+\}/g, " ").match(/[\p{L}]+/gu) ?? []) {
          const w = word.toLowerCase();
          const set = spellings.get(plain(w)) ?? new Set();
          spellings.set(plain(w), set.add(w));
        }
      }
      // Known pairs that are different words by meaning: si/sí, tu/tú, mas/más, el/él, se/sé, te/té, de/dé, que/qué, como/cómo,
      // cual/cuál, donde/dónde, cuando/cuándo, ou/où, a/à, la/là, sur/sûr, du/dû.
      const different = new Set(["si", "tu", "mas", "el", "se", "te", "de", "que", "como", "cual", "donde", "cuando", "quien", "cuanto", "ou", "a", "la", "sur", "du", "mur", "ce", "cote", "pecher", "peche"]);
      const bad = [...spellings].filter(([base, set]) => set.size > 1 && !different.has(base)).map(([base, set]) => `${lang}: ${base} → ${[...set].join(" / ")}`);
      expect(bad).toEqual([]);
    }
  });

  it("opens Spanish questions and exclamations with ¿ and ¡", () => {
    for (const [key, text] of Object.entries(messages.es)) {
      for (const sentence of text.split(/(?<=[.?!])\s+/)) {
        if (sentence.includes("?")) expect(sentence, `es ${key}`).toContain("¿");
        if (sentence.includes("!")) expect(sentence, `es ${key}`).toContain("¡");
      }
    }
  });
});
