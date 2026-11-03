// Rebuilds latest.json (what Talaan's updater reads) from every updater signature on a GitHub release, then
// uploads it. Each platform uploads its own files at its own time (Windows on CI, the Mac from this Mac), and
// whoever runs this last writes the full file. Two runs at once could still race (one uploads a file built before
// the other's .sig landed), so after uploading it reads latest.json and the .sig list back and, if a signature is
// missing from the file, rebuilds and uploads again (up to 3 tries, 5 s apart). Still: run the Mac upload after
// the CI updater-json job has finished (docs/TESTING.md, Before publishing).
//   node scripts/latest-json.mjs v1.0.0            uses the draft or published release for that tag (gh CLI)
//   node scripts/latest-json.mjs v1.0.0 --dir DIR  prints the file built from DIR/*.sig, uploads nothing
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { changelogSection, plainNotes } from "./release-notes.mjs";

export function platformsFor(name) {
  if (/_x64-setup\.exe\.sig$/.test(name)) return ["windows-x86_64", "windows-x86_64-nsis"];
  if (/\.app\.tar\.gz\.sig$/.test(name)) return /x86_64|_x64/.test(name) ? ["darwin-x86_64", "darwin-x86_64-app"] : ["darwin-aarch64", "darwin-aarch64-app"];
  return [];
}

export function buildLatest({ repo, tag, notes, pubDate, sigs }) {
  const platforms = {};
  for (const { name, signature } of [...sigs].sort((a, b) => a.name.localeCompare(b.name))) {
    const url = `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name.slice(0, -".sig".length))}`;
    for (const key of platformsFor(name)) platforms[key] = { signature: signature.trim(), url };
  }
  if (Object.keys(platforms).length === 0) throw new Error("no updater signatures on the release");
  return { version: tag.replace(/^v/, ""), notes, pub_date: pubDate, platforms };
}

/**
 * The updater's `notes`: this version's CHANGELOG.md section as plain text (cut to a sane length, with a link to the
 * full release notes), or just the link when the changelog has no section (release.yml's draft job already fails
 * then; this keeps a hand-run upload working).
 */
export function updaterNotes({ changelog, repo, tag }) {
  const link = `https://github.com/${repo}/releases/tag/${tag}`;
  try {
    return plainNotes(changelogSection(changelog, tag), { moreUrl: link });
  } catch {
    return `What's new: ${link}`;
  }
}

/** Platform keys that `sigs` (the release's .sig files) call for but `latest` lacks or signs differently. */
export function missingPlatforms(latest, sigs) {
  const missing = [];
  for (const { name, signature } of sigs) {
    for (const key of platformsFor(name)) {
      if (latest?.platforms?.[key]?.signature !== signature.trim()) missing.push(key);
    }
  }
  return missing.sort();
}

const gh = (...args) => execFileSync("gh", args, { encoding: "utf8" });
const TRIES = 3;
const WAIT_MS = 5000;

function readSigs(dir) {
  return readdirSync(dir).filter((n) => n.endsWith(".sig")).map((name) => ({ name, signature: readFileSync(join(dir, name), "utf8") }));
}

/** A fresh folder with the release's .sig files (and latest.json when `withLatest`). */
function download(repo, tag, withLatest) {
  const dir = mkdtempSync(join(tmpdir(), "talaan-latest-"));
  const patterns = withLatest ? ["*.sig", "latest.json"] : ["*.sig"];
  gh("release", "download", tag, "--repo", repo, ...patterns.flatMap((p) => ["--pattern", p]), "--dir", dir);
  return dir;
}

async function main([tag, flag, dir]) {
  if (!tag) throw new Error("usage: node scripts/latest-json.mjs v1.0.0 [--dir DIR]");
  const repo = process.env.GITHUB_REPOSITORY || "mmnzns/Talaan";
  const notes = updaterNotes({ changelog: readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8"), repo, tag });
  const pubDate = new Date().toISOString();
  if (flag === "--dir") return console.log(JSON.stringify(buildLatest({ repo, tag, notes, pubDate, sigs: readSigs(dir) }), null, 2));
  for (let attempt = 1; ; attempt++) {
    const from = download(repo, tag, false);
    const json = JSON.stringify(buildLatest({ repo, tag, notes, pubDate, sigs: readSigs(from) }), null, 2);
    const file = join(from, "latest.json");
    writeFileSync(file, json);
    gh("release", "upload", tag, file, "--repo", repo, "--clobber");
    // Read it back: another upload may have landed a .sig (or its own latest.json) meanwhile.
    await new Promise((r) => setTimeout(r, WAIT_MS));
    const back = download(repo, tag, true);
    const missing = missingPlatforms(JSON.parse(readFileSync(join(back, "latest.json"), "utf8")), readSigs(back));
    if (missing.length === 0) return console.log(`latest.json: ${Object.keys(JSON.parse(json).platforms).join(", ")}`);
    if (attempt === TRIES) throw new Error(`latest.json still lacks ${missing.join(", ")} after ${TRIES} tries; run this script again`);
    console.log(`latest.json lacks ${missing.join(", ")}; rebuilding (try ${attempt + 1} of ${TRIES})`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((e) => {
    console.error(e.message);
    process.exit(1);
  });
}
