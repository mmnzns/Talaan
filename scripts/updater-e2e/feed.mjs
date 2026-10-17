// Updater end-to-end test: lays out a local copy of a GitHub release for serve.mjs. latest.json comes from the real
// scripts/latest-json.mjs (its --dir mode, run as a command like the release does), then only the origin of each
// url changes (https://github.com/ -> http://127.0.0.1:<port>/), so its file names and paths are what users get.
//   node scripts/updater-e2e/feed.mjs <folder with the update files and .sigs> <tag> <serve root> <port> <platform key>...
// Writes <serve root>/<repo>/releases/download/<tag>/<file> and <serve root>/<repo>/releases/latest/download/latest.json,
// and fails unless latest.json has every <platform key> given.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export function localize(latest, port) {
  const platforms = {};
  for (const [key, p] of Object.entries(latest.platforms)) {
    if (!p.url.startsWith("https://github.com/")) throw new Error(`unexpected url for ${key}: ${p.url}`);
    platforms[key] = { ...p, url: `http://127.0.0.1:${port}/${p.url.slice("https://github.com/".length)}` };
  }
  return { ...latest, platforms };
}

function main([from, tag, root, port, ...keys]) {
  if (!port) throw new Error("usage: node scripts/updater-e2e/feed.mjs <folder> <tag> <serve root> <port> <platform key>...");
  const repo = process.env.GITHUB_REPOSITORY || "mmnzns/Talaan";
  const script = join(dirname(fileURLToPath(import.meta.url)), "..", "latest-json.mjs");
  const latest = JSON.parse(execFileSync(process.execPath, [script, tag, "--dir", from], { encoding: "utf8" }));
  const missing = keys.filter((k) => !latest.platforms[k]);
  if (missing.length) throw new Error(`latest.json lacks ${missing.join(", ")}: ${JSON.stringify(latest)}`);
  const files = join(root, repo, "releases", "download", tag);
  mkdirSync(files, { recursive: true });
  for (const sig of readdirSync(from).filter((n) => n.endsWith(".sig"))) {
    const update = sig.slice(0, -".sig".length);
    if (!existsSync(join(from, update))) throw new Error(`${sig} has no ${update} beside it`);
    copyFileSync(join(from, update), join(files, update));
    copyFileSync(join(from, sig), join(files, sig));
  }
  const local = localize(latest, port);
  const at = join(root, repo, "releases", "latest", "download");
  mkdirSync(at, { recursive: true });
  writeFileSync(join(at, "latest.json"), JSON.stringify(local, null, 2));
  console.log(JSON.stringify(local, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
