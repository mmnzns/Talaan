// Updater end-to-end test (.github/workflows/updater-test.yml): writes the config overlay for one test build.
// Merged last with `tauri build --config`, so tauri.conf.json itself never changes. It sets a test version, a
// throwaway updater public key (made in the CI job), the local feed on 127.0.0.1 (plain http, which the updater
// refuses in release builds unless dangerousInsecureTransportProtocol is on) and the updater-e2e feature
// (src-tauri/src/commands/updater_e2e.rs).
//   node scripts/updater-e2e/config.mjs <version> <public key file> <endpoint> <out.json> [--mac]
// --mac also ad-hoc signs the app ("-"), so it runs on Apple silicon without a certificate.
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export function overlay({ version, pubkey, endpoint, mac }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`not a plain x.y.z version: ${version}`);
  if (!/^http:\/\/127\.0\.0\.1:\d+\//.test(endpoint)) throw new Error(`the test feed must be on 127.0.0.1: ${endpoint}`);
  const bundle = { createUpdaterArtifacts: true };
  if (mac) bundle.macOS = { signingIdentity: "-" };
  return {
    version,
    build: { features: ["updater-e2e"] },
    bundle,
    plugins: { updater: { pubkey: pubkey.trim(), endpoints: [endpoint], dangerousInsecureTransportProtocol: true } },
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [version, pubFile, endpoint, out, flag] = process.argv.slice(2);
  if (!out) {
    console.error("usage: node scripts/updater-e2e/config.mjs <version> <public key file> <endpoint> <out.json> [--mac]");
    process.exit(1);
  }
  try {
    writeFileSync(out, JSON.stringify(overlay({ version, pubkey: readFileSync(pubFile, "utf8"), endpoint, mac: flag === "--mac" }), null, 2));
    console.log(`${out}: version ${version}, feed ${endpoint}`);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
