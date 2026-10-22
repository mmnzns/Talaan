// Release notes from CHANGELOG.md: the section for one version, ready for a GitHub release body (soft-wrapped lines
// joined, because a release body keeps every newline) or as plain text for latest.json's `notes`.
//   node scripts/release-notes.mjs v1.1.1          the full release body (what's new + scripts/release-notes.md)
//   node scripts/release-notes.mjs v1.1.1 --plain  plain text for latest.json
// Exits 1 when CHANGELOG.md has no section for the version, so the release job fails before a draft goes out
// without notes.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const HEADING = /^##\s+\[?v?(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)\]?(?:\s|$)/;

/** The text of `## <version> (...)` up to the next `## ` heading, trimmed. Throws when there is none or it is empty. */
export function changelogSection(markdown, version) {
  const want = version.replace(/^v/, "");
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const start = lines.findIndex((l) => HEADING.exec(l)?.[1] === want);
  if (start === -1) throw new Error(`CHANGELOG.md has no section for ${want} (expected a "## ${want} (date)" heading)`);
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => /^##\s/.test(l));
  const section = (end === -1 ? rest : rest.slice(0, end)).join("\n").trim();
  if (!section) throw new Error(`CHANGELOG.md has a heading for ${want} but nothing under it`);
  return section;
}

const FENCE = /^\s*(```|~~~)/;
const BLOCK_START = /^\s*(?:[-*+]\s|\d+[.)]\s|#{1,6}\s|>|\||<)/;

/**
 * Joins soft-wrapped lines: a line that continues the paragraph or list item above it is appended to it with one
 * space. List items, headings, quotes, table rows, code fences, blank lines and hard breaks (two trailing spaces or a
 * backslash) stay as they are. Bold, lists and everything else is kept.
 */
export function joinSoftWraps(text) {
  const out = [];
  let inFence = false;
  let canJoin = false; // the previous output line is text a following line could continue
  for (const raw of text.replace(/\r\n?/g, "\n").split("\n")) {
    if (FENCE.test(raw)) {
      inFence = !inFence;
      out.push(raw.trimEnd());
      canJoin = false;
      continue;
    }
    if (inFence) {
      out.push(raw);
      continue;
    }
    const line = raw.trimEnd();
    if (!line.trim()) {
      if (out.length && out[out.length - 1] !== "") out.push("");
      canJoin = false;
      continue;
    }
    const hardBreak = / {2,}$/.test(raw) || line.endsWith("\\");
    if (canJoin && !BLOCK_START.test(line)) {
      out[out.length - 1] += ` ${line.trim()}`;
    } else {
      out.push(line);
    }
    // Headings and table rows never take a continuation; everything else may, unless the line ends in a hard break.
    canJoin = !hardBreak && !/^\s*(?:#{1,6}\s|\|)/.test(line);
  }
  while (out.length && out[out.length - 1] === "") out.pop();
  return out.join("\n");
}

/** The release body: what's new for this version, then the install text (scripts/release-notes.md with {{TAG}}). */
export function releaseBody({ changelog, tag, template }) {
  const version = tag.replace(/^v/, "");
  const news = joinSoftWraps(changelogSection(changelog, version));
  return `## What's new in ${version}\n\n${news}\n\n---\n\n${template.replaceAll("{{TAG}}", tag).trim()}\n`;
}

/** Markdown to plain text for the updater's `notes`: no emphasis, links as their text, headings as lines. */
export function toPlain(markdown) {
  return markdown
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/(\*\*|__)(?=\S)(.+?)(?<=\S)\1/g, "$2")
    .replace(/(?<![\w*])\*(?=[^\s*])([^*\n]+?)(?<=[^\s*])\*(?![\w*])/g, "$1")
    .replace(/`([^`\n]*)`/g, "$1")
    .replace(/^(\s*)[*+]\s/gm, "$1- ")
    .replace(/[ \t]+$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/** Plain notes for latest.json, cut at a whole line under `max` characters with a pointer to the full notes. */
export function plainNotes(section, { max = 1200, moreUrl } = {}) {
  const text = toPlain(joinSoftWraps(section));
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const lineEnd = cut.lastIndexOf("\n");
  let kept = (lineEnd > max / 2 ? cut.slice(0, lineEnd) : cut.replace(/\s+\S*$/, "")).trimEnd();
  // Don't end on a group title ("Around the meeting") whose items were cut off.
  const lines = kept.split("\n");
  while (lines.length > 1 && !/^\s*(?:[-+]|\d+[.)])\s/.test(lines[lines.length - 1])) lines.pop();
  kept = lines.join("\n").trimEnd();
  return moreUrl ? `${kept}\n…and more: ${moreUrl}` : `${kept}…`;
}

function main([tag, flag]) {
  if (!tag) throw new Error("usage: node scripts/release-notes.mjs v1.1.1 [--plain]");
  const changelog = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8");
  if (flag === "--plain") return process.stdout.write(`${plainNotes(changelogSection(changelog, tag))}\n`);
  const template = readFileSync(new URL("./release-notes.md", import.meta.url), "utf8");
  process.stdout.write(releaseBody({ changelog, tag, template }));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`::error::${e.message}`);
    process.exit(1);
  }
}
