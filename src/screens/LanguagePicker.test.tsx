import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { LanguagePicker } from "./LanguagePicker";

// There is no browser DOM in the tests, so these render the picker's first state to HTML. What the keys and clicks
// do rests on the search (lib/summaryLanguages.test.ts) and on the buttons' own behavior.
const html = (props: Partial<Parameters<typeof LanguagePicker>[0]> = {}) => renderToStaticMarkup(<LanguagePicker onPick={() => {}} {...props} />);
const option = (markup: string, code: string) => markup.match(new RegExp(`<button[^>]*data-code="${code}"[^>]*>.*?</button>`))?.[0] ?? "";

describe("the language picker", () => {
  it("lists the 32 languages with a search box, each with its name and its own name", () => {
    const markup = html();
    expect(markup.match(/role="option"/g)).toHaveLength(32);
    expect(markup).toContain('type="search"');
    expect(markup).toContain("Japanese · 日本語");
    expect(markup).toContain("Filipino");
    expect(markup).toContain("Chinese");
  });

  it("marks the current language, and only it", () => {
    const markup = html({ value: "fil" });
    expect(markup.match(/aria-selected="true"/g)).toHaveLength(1);
    expect(option(markup, "fil")).toContain('aria-selected="true"');
    expect(option(markup, "fil")).toContain("current");
    expect(option(markup, "ja")).toContain('aria-selected="false"');
  });

  it("puts extra rows (Same as meeting) first", () => {
    const markup = html({ value: "meeting", extra: [{ code: "meeting", label: "Same as meeting" }] });
    expect(markup.match(/role="option"/g)).toHaveLength(33);
    expect(markup.indexOf("Same as meeting")).toBeLessThan(markup.indexOf("English"));
    expect(option(markup, "meeting")).toContain('aria-selected="true"');
  });

  it("says 'works best with online AI' only for languages the chosen local model wrote poorly", () => {
    const markup = html({ localModel: "qwen3.5-4b" });
    expect(option(markup, "fil")).toContain("online AI");
    expect(option(markup, "en")).not.toContain("online AI");
    expect(html({ localModel: "qwen3.5-9b" })).not.toContain("online AI");
    expect(html({})).not.toContain("online AI");
  });
});
