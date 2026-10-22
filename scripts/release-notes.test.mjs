import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { updaterNotes } from "./latest-json.mjs";
import { changelogSection, joinSoftWraps, plainNotes, releaseBody, toPlain } from "./release-notes.mjs";

const CHANGELOG = `# Changelog

## 1.2.0 (2026-11-01)

- **New thing:** a long line that was
  wrapped by hand in the editor and
  continues here.
- **Another:** short.

**Grouped**
- Child one
- Child two
  with a wrapped tail

## 1.1.1 (2026-10-19)

- **Know:** the old one.

## 1.1.0 (2026-10-17)

- Old.
`;

describe("changelogSection", () => {
  it("returns the version's section up to the next heading", () => {
    const s = changelogSection(CHANGELOG, "1.1.1");
    expect(s).toBe("- **Know:** the old one.");
  });
  it("accepts a tag with a v", () => {
    expect(changelogSection(CHANGELOG, "v1.1.0")).toBe("- Old.");
  });
  it("does not match a longer or shorter version", () => {
    expect(() => changelogSection(CHANGELOG, "1.1")).toThrow(/no section for 1\.1/);
    expect(() => changelogSection("## 1.1.10 (x)\n- a\n", "1.1.1")).toThrow(/no section/);
  });
  it("fails when the version has no section", () => {
    expect(() => changelogSection(CHANGELOG, "9.9.9")).toThrow(/no section for 9\.9\.9/);
  });
  it("fails when the section is empty", () => {
    expect(() => changelogSection("## 2.0.0 (x)\n\n\n## 1.0.0 (y)\n- a\n", "2.0.0")).toThrow(/nothing under it/);
  });
  it("reads the last section and handles Windows line endings and bracketed headings", () => {
    expect(changelogSection("## [3.0.0] - 2026\r\n- x\r\n- y\r\n", "3.0.0")).toBe("- x\n- y");
  });
});

describe("joinSoftWraps", () => {
  it("joins wrapped list items and keeps list items and bold", () => {
    const out = joinSoftWraps(changelogSection(CHANGELOG, "1.2.0"));
    expect(out).toBe(
      [
        "- **New thing:** a long line that was wrapped by hand in the editor and continues here.",
        "- **Another:** short.",
        "",
        "**Grouped**",
        "- Child one",
        "- Child two with a wrapped tail",
      ].join("\n"),
    );
  });
  it("joins a wrapped paragraph but not across a blank line", () => {
    expect(joinSoftWraps("one\ntwo\n\nthree")).toBe("one two\n\nthree");
  });
  it("keeps headings, nested items, table rows and hard breaks", () => {
    expect(joinSoftWraps("### Fixed\ntext\n  - nested\n  - more\n| a | b |\n| c | d |")).toBe("### Fixed\ntext\n  - nested\n  - more\n| a | b |\n| c | d |");
    expect(joinSoftWraps("line one  \nline two")).toBe("line one\nline two");
    expect(joinSoftWraps("line one\\nline two")).toBe("line one\\nline two");
  });
  it("leaves code blocks alone", () => {
    expect(joinSoftWraps("```\na\nb\n```\nafter\nmore")).toBe("```\na\nb\n```\nafter more");
  });
  it("collapses blank runs and trims the ends", () => {
    expect(joinSoftWraps("\n\na\n\n\n\nb\n\n")).toBe("a\n\nb");
  });
});

describe("releaseBody", () => {
  const template = "**First install**\n\nSee [INSTALL](https://x/blob/{{TAG}}/docs/INSTALL.md).\n";
  it("puts the version's notes first, then the install text for the tag", () => {
    const body = releaseBody({ changelog: CHANGELOG, tag: "v1.1.1", template });
    expect(body.startsWith("## What's new in 1.1.1\n\n- **Know:** the old one.\n\n---\n\n**First install**")).toBe(true);
    expect(body).toContain("/blob/v1.1.1/docs/INSTALL.md");
    expect(body).not.toContain("{{TAG}}");
    expect(body.endsWith("\n")).toBe(true);
  });
  it("throws without a section", () => {
    expect(() => releaseBody({ changelog: CHANGELOG, tag: "v7.0.0", template })).toThrow(/no section/);
  });
});

describe("plain text for latest.json", () => {
  it("drops emphasis, links and headings", () => {
    expect(toPlain("### Fixed\n- **Bold:** a `code` and [a link](https://x.y) and *it*.\n* star item")).toBe("Fixed\n- Bold: a code and a link and it.\n- star item");
  });
  it("keeps star-wrapped words that are not emphasis around a list marker", () => {
    expect(toPlain("- 2 * 3 * 4")).toBe("- 2 * 3 * 4");
  });
  it("is cut at a whole line with a pointer to the full notes when long", () => {
    const section = Array.from({ length: 40 }, (_, i) => `- Item number ${i} with some words to fill the line up`).join("\n");
    const out = plainNotes(section, { max: 300, moreUrl: "https://example.com/r" });
    expect(out.length).toBeLessThan(380);
    expect(out.endsWith("…and more: https://example.com/r")).toBe(true);
    const lines = out.split("\n").slice(0, -1);
    expect(lines.every((l) => /^- Item number \d+ with some words to fill the line up$/.test(l))).toBe(true);
  });
  it("does not end on a group title whose items were cut off", () => {
    const section = "**First group**\n- one item here\n- two items here\n\n**Second group**\n- three items here and more words";
    const out = plainNotes(section, { max: 62 });
    expect(out).toBe("First group\n- one item here\n- two items here…");
  });
  it("leaves a short section whole", () => {
    expect(plainNotes("- **a**\n- b", { max: 300, moreUrl: "u" })).toBe("- a\n- b");
  });
});

describe("updaterNotes", () => {
  const link = "https://github.com/mmnzns/Talaan/releases/tag/v1.1.1";
  it("is the plain section", () => {
    expect(updaterNotes({ changelog: CHANGELOG, repo: "mmnzns/Talaan", tag: "v1.1.1" })).toBe("- Know: the old one.");
  });
  it("falls back to the link when the changelog has no section", () => {
    expect(updaterNotes({ changelog: "# Changelog\n", repo: "mmnzns/Talaan", tag: "v1.1.1" })).toBe(`What's new: ${link}`);
  });
});

describe("this repository", () => {
  const changelog = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8");
  const version = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8")).version;

  it("has a CHANGELOG section for the version in tauri.conf.json (the release job fails without one)", () => {
    expect(() => changelogSection(changelog, version)).not.toThrow();
  });

  it("builds a body for the current version from the real files, with no soft wraps left", () => {
    const body = releaseBody({ changelog, tag: `v${version}`, template: readFileSync(new URL("./release-notes.md", import.meta.url), "utf8") });
    expect(body).toContain(`## What's new in ${version}`);
    expect(body).toContain("/blob/v" + version + "/docs/INSTALL.md");
  });

  it("the command line prints the body, and exits 1 with an error for an unknown version", () => {
    const script = new URL("./release-notes.mjs", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
    const ok = execFileSync("node", [script, `v${version}`], { encoding: "utf8" });
    expect(ok).toContain(`What's new in ${version}`);
    const plain = execFileSync("node", [script, `v${version}`, "--plain"], { encoding: "utf8" });
    expect(plain).not.toContain("**");
    const bad = spawnSync("node", [script, "v0.0.1"], { encoding: "utf8" });
    expect(bad.status).toBe(1);
    expect(bad.stderr).toContain("::error::");
    expect(bad.stdout).toBe("");
  });
});
