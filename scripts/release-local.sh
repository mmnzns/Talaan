#!/bin/bash
# Builds a signed release on this Mac (when the GitHub release workflow can't sign: its certificate secrets
# aren't set). Signs the app with the self-signed certificate and the updater files with Talaan's key
# (~/.tauri/talaan.key, password from the login Keychain). Output: the .dmg, Talaan.app.tar.gz and its .sig.
set -euo pipefail
cd "$(dirname "$0")/.."
IDENTITY="${APPLE_SIGNING_IDENTITY:-Parlato Code Signing}"
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/talaan.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -a talaan-updater -s 'Talaan updater key password' -w)"
CONF=$(mktemp)
node -e "const fs=require('fs'),c=JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json'));c.bundle.createUpdaterArtifacts=true;fs.writeFileSync('$CONF',JSON.stringify(c))"
APPLE_SIGNING_IDENTITY="$IDENTITY" npm run tauri build -- --target aarch64-apple-darwin --config "$CONF"
rm -f "$CONF"
OUT=src-tauri/target/aarch64-apple-darwin/release/bundle
codesign --verify --deep --strict "$OUT/macos/Talaan.app" && echo "signature ok"
ls -1 "$OUT"/dmg/*.dmg "$OUT"/macos/*.tar.gz "$OUT"/macos/*.sig
