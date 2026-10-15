// Rebuilds latest.json (what Talaan's updater reads) from every updater signature on a GitHub release, then
// uploads it. Each platform uploads its own files at its own time (Windows on CI, the Mac from this Mac), and
// whoever runs this last writes the full file, so no platform's entry is lost.
//   node scripts/latest-json.mjs v1.0.0            uses the draft or published release for that tag (gh CLI)
//   node scripts/latest-json.mjs v1.0.0 --dir DIR  prints the file built from DIR/*.sig, uploads nothing
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

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

const gh = (...args) => execFileSync("gh", args, { encoding: "utf8" });

function main([tag, flag, dir]) {
  if (!tag) throw new Error("usage: node scripts/latest-json.mjs v1.0.0 [--dir DIR]");
  const repo = process.env.GITHUB_REPOSITORY || "mmnzns/Talaan";
  const notes = `What's new: https://github.com/${repo}/releases/tag/${tag}`;
  const pubDate = new Date().toISOString();
  const local = flag === "--dir";
  const from = local ? dir : mkdtempSync(join(tmpdir(), "talaan-latest-"));
  if (!local) gh("release", "download", tag, "--repo", repo, "--pattern", "*.sig", "--dir", from, "--clobber");
  const sigs = readdirSync(from).filter((n) => n.endsWith(".sig")).map((name) => ({ name, signature: readFileSync(join(from, name), "utf8") }));
  const json = JSON.stringify(buildLatest({ repo, tag, notes, pubDate, sigs }), null, 2);
  if (local) return console.log(json);
  const file = join(from, "latest.json");
  writeFileSync(file, json);
  gh("release", "upload", tag, file, "--repo", repo, "--clobber");
  console.log(`latest.json: ${Object.keys(JSON.parse(json).platforms).join(", ")}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main(process.argv.slice(2));
