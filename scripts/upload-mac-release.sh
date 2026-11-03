#!/bin/bash
# Uploads the Mac build from scripts/release-local.sh to the GitHub release for TAG (draft or not; it makes the
# draft if CI hasn't yet), then rebuilds latest.json from every updater signature on it, so the Windows entry from
# CI stays. Usage: scripts/upload-mac-release.sh v1.0.0
# Run it after the CI updater-json job has finished (Actions tab), so two latest.json uploads don't race; it ends by
# checking that latest.json has all four platform keys.
set -euo pipefail
cd "$(dirname "$0")/.."
TAG="${1:?usage: scripts/upload-mac-release.sh v1.0.0}"
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
[ "$TAG" = "v$VERSION" ] || { echo "tag $TAG but tauri.conf.json says $VERSION" >&2; exit 1; }
OUT=src-tauri/target/aarch64-apple-darwin/release/bundle
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
# Same names tauri-action uses, so a CI-built Mac release looks the same.
cp "$OUT/macos/Talaan.app.tar.gz" "$TMP/Talaan_aarch64.app.tar.gz"
cp "$OUT/macos/Talaan.app.tar.gz.sig" "$TMP/Talaan_aarch64.app.tar.gz.sig"
# This version's CHANGELOG.md section plus the install text (fails when CHANGELOG.md has no section for it).
node scripts/release-notes.mjs "$TAG" > "$TMP/notes.md"
gh release view "$TAG" >/dev/null 2>&1 || gh release create "$TAG" --draft --verify-tag --title "Talaan $TAG" --notes-file "$TMP/notes.md"
gh release upload "$TAG" "$OUT/dmg/Talaan_${VERSION}_aarch64.dmg" "$TMP/Talaan_aarch64.app.tar.gz" "$TMP/Talaan_aarch64.app.tar.gz.sig" --clobber
node scripts/latest-json.mjs "$TAG"
gh release download "$TAG" --pattern latest.json --dir "$TMP/check"
KEYS=$(node -p "Object.keys(JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8')).platforms).sort().join(' ')" "$TMP/check/latest.json")
WANT="darwin-aarch64 darwin-aarch64-app windows-x86_64 windows-x86_64-nsis"
if [ "$KEYS" != "$WANT" ]; then
  echo "latest.json has [$KEYS], expected [$WANT]. Wait for the CI updater-json job, then run: node scripts/latest-json.mjs $TAG" >&2
  exit 1
fi
echo "latest.json has all four platform keys."
