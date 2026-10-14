#!/bin/bash
# Builds a signed debug Talaan.app for testing permissions (microphone, system audio) the way users get them.
# Uses the free self-signed "Parlato Code Signing" certificate, so macOS keeps the permissions across rebuilds.
# The first run may ask to let codesign use the key: choose Always Allow.
set -euo pipefail
cd "$(dirname "$0")/.."
IDENTITY="${APPLE_SIGNING_IDENTITY:-Parlato Code Signing}"
APPLE_SIGNING_IDENTITY="$IDENTITY" npm run tauri build -- --debug --bundles app
APP="src-tauri/target/debug/bundle/macos/Talaan.app"
codesign --verify --deep --strict "$APP" && echo "signature ok"
codesign -d --entitlements - "$APP" 2>/dev/null | grep -q "device.audio-input" && echo "microphone entitlement ok"
/usr/libexec/PlistBuddy -c "Print :NSAudioCaptureUsageDescription" "$APP/Contents/Info.plist" >/dev/null && echo "system audio usage string ok"
echo "$APP"
