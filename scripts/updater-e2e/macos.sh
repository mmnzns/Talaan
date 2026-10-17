#!/bin/bash
# Updater end-to-end test, Mac part (.github/workflows/updater-test.yml; docs/TESTING.md). Runs on a disposable
# GitHub runner only: it installs Talaan in /Applications and writes ~/Library/Application Support/Talaan.
# Never run it on a Mac that has a real Talaan.
#
# Installs build A (an ad-hoc signed Talaan.app), seeds a meeting and settings (Open at login on), starts A with
# TALAAN_UPDATER_E2E=install (the updater-e2e hook runs the same update check and install the Settings buttons
# run), and checks that the updater replaced the app with B, B restarted by itself, and the meeting, settings and
# Open at login survived. Every check runs; the script fails at the end if any did. Logs and reports go to OUT.
#   scripts/updater-e2e/macos.sh <A's Talaan.app> <version A> <version B> <feed root> <port> <out folder>
set -uo pipefail
cd "$(dirname "$0")/../.."
APP_A="$1"; VERSION_A="$2"; VERSION_B="$3"; FEED_ROOT="$4"; PORT="$5"; OUT="$6"
mkdir -p "$OUT"
FAILED=()
pass() { echo "ok: $1"; }
fail() { echo "::error::$1"; FAILED+=("$1"); }
check() { if eval "$1"; then pass "$2"; else fail "$2"; fi; }

APP=/Applications/Talaan.app
DATA="$HOME/Library/Application Support/Talaan"
LOGS="$HOME/Library/Logs/Talaan"
AGENT="$HOME/Library/LaunchAgents/Talaan.plist"
export MEETING_ID=2026-10-01_0900_upd1
MEETING_DIR="$DATA/meetings/$MEETING_ID"
version_of() { /usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>/dev/null; }
# report <version> <js expression on r>: prints the value, or nothing when the report isn't there yet.
report() { local f="$LOGS/updater-e2e-$1.json"; [ -f "$f" ] && node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));const v=('"$2"');process.stdout.write(typeof v==="string"?v:JSON.stringify(v??null))' "$f" 2>/dev/null; }
wait_for() { local secs="$1"; shift; local end=$((SECONDS + secs)); while [ $SECONDS -lt $end ]; do "$@" && return 0; sleep 1; done; return 1; }
has_report() { [ -f "$LOGS/updater-e2e-$1.json" ]; }
save_logs() {
  [ -d "$LOGS" ] && cp -R "$LOGS" "$OUT/app-logs"
  [ -f "$DATA/settings.json" ] && cp "$DATA/settings.json" "$OUT/settings.after.json"
  [ -f "$AGENT" ] && cp "$AGENT" "$OUT/Talaan.plist"
  ps -axo pid,command | grep -i '[t]alaan' > "$OUT/processes.txt" || true
}

# 1. Install A.
rm -rf "$APP"
ditto "$APP_A" "$APP"
BIN=$(find "$APP/Contents/MacOS" -maxdepth 1 -type f ! -name 'talaan-llm*' | head -1)
check '[ -x "$BIN" ]' "the app binary is $BIN"
check '[ "$(version_of)" = "$VERSION_A" ]' "Talaan.app is $VERSION_A before the update ($(version_of))"
codesign -dv "$APP" > "$OUT/codesign-a.txt" 2>&1 || true

# 2. Seed a meeting and settings, the way an existing user has them.
mkdir -p "$MEETING_DIR"
cat > "$MEETING_DIR/meeting.json" <<JSON
{"schemaVersion":1,"id":"$MEETING_ID","title":"Updater test meeting","app":"zoom","appLabel":"Zoom","startedAt":"2026-10-01T09:00:00-07:00","durationMs":60000,"language":"en","status":"ready","pinned":true}
JSON
printf '# Updater test meeting\n\nMe: this file must survive the update.\n' > "$MEETING_DIR/transcript.md"
echo '{"onboardingDone":true,"yourName":"Updater Test","launchAtLogin":true,"checkUpdates":true,"theme":"dark"}' > "$DATA/settings.json"
TRANSCRIPT_HASH=$(shasum -a 256 "$MEETING_DIR/transcript.md" | cut -d' ' -f1)
check '[ ! -f "$AGENT" ]' "no Open at login entry before A starts"

# 3. The feed: latest.json and B's .app.tar.gz on 127.0.0.1.
node scripts/updater-e2e/serve.mjs "$FEED_ROOT" "$PORT" > "$OUT/feed-server.log" 2>&1 &
SERVER=$!
check 'wait_for 20 curl -s -o /dev/null "http://127.0.0.1:$PORT/"' "feed server is up on $PORT"

# 4. Start A; its hook checks, downloads, verifies and installs B, then Talaan restarts as B.
TALAAN_UPDATER_E2E=install "$BIN" > "$OUT/app.stdout.log" 2>&1 &
if ! wait_for 180 has_report "$VERSION_A"; then fail "A wrote no updater-e2e-$VERSION_A.json within 3 minutes"; save_logs; kill $SERVER; exit 1; fi
check '[ "$(report "$VERSION_A" "r.meetings.includes(process.env.MEETING_ID)")" = true ]' "A lists the seeded meeting"
check '[ "$(report "$VERSION_A" r.autostart)" = true ]' "A turned Open at login on (autostart: $(report "$VERSION_A" r.autostart))"
wait_for 420 has_report "$VERSION_B"; GOT_B=$?
cp "$LOGS/updater-e2e-$VERSION_A.json" "$OUT/report-$VERSION_A.json" 2>/dev/null
check '[ "$(report "$VERSION_A" "r.check && r.check.found && r.check.found.version")" = "$VERSION_B" ]' "A found $VERSION_B (check: $(report "$VERSION_A" r.check))"
check '[ "$(report "$VERSION_A" "r.install && r.install.error")" = null ]' "A's install didn't fail ($(report "$VERSION_A" r.install))"
check '[ $GOT_B -eq 0 ]' "Talaan restarted by itself as $VERSION_B after the update"
if [ $GOT_B -ne 0 ] && [ "$(version_of)" = "$VERSION_B" ]; then
  # Finish the other checks anyway: start B by hand.
  "$BIN" >> "$OUT/app.stdout.log" 2>&1 &
  wait_for 120 has_report "$VERSION_B"; GOT_B=$?
fi

# 5. After the update.
check '[ "$(version_of)" = "$VERSION_B" ]' "Talaan.app is $VERSION_B after the update ($(version_of))"
codesign -dv "$APP" > "$OUT/codesign-b.txt" 2>&1 || true
if [ $GOT_B -eq 0 ]; then
  cp "$LOGS/updater-e2e-$VERSION_B.json" "$OUT/report-$VERSION_B.json"
  check '[ "$(report "$VERSION_B" r.version)" = "$VERSION_B" ]' "B reports version $(report "$VERSION_B" r.version)"
  check '[ "$(report "$VERSION_B" "r.meetings.includes(process.env.MEETING_ID)")" = true ]' "B lists the seeded meeting"
  check '[ "$(report "$VERSION_B" r.settings.launchAtLogin)" = true ]' "B reads Open at login = on from settings"
  check '[ "$(report "$VERSION_B" r.settings.yourName)" = "Updater Test" ]' "B reads the seeded name"
  check '[ "$(report "$VERSION_B" r.autostart)" = true ]' "B says Open at login is on (autostart: $(report "$VERSION_B" r.autostart))"
fi
check '[ -f "$MEETING_DIR/meeting.json" ]' "meeting.json is still there"
check '[ "$(shasum -a 256 "$MEETING_DIR/transcript.md" 2>/dev/null | cut -d" " -f1)" = "$TRANSCRIPT_HASH" ]' "transcript.md is unchanged"
check 'node -e "const s=JSON.parse(require(\"fs\").readFileSync(process.argv[1],\"utf8\"));process.exit(s.launchAtLogin===true&&s.yourName===\"Updater Test\"?0:1)" "$DATA/settings.json"' "settings.json still has the seeded values"
check 'grep -q "$APP" "$AGENT" 2>/dev/null' "the login item ($AGENT) points at $APP"
check 'grep -Eq "GET [^ ]*\.app\.tar\.gz 200" "$OUT/feed-server.log"' "B's .app.tar.gz was downloaded from the feed"

# 6. Clean up and report.
pkill -f "$APP/Contents/MacOS" || true
kill $SERVER 2>/dev/null || true
save_logs
if [ ${#FAILED[@]} -gt 0 ]; then
  echo; echo "${#FAILED[@]} check(s) failed:"; printf '  - %s\n' "${FAILED[@]}"
  exit 1
fi
echo; echo "Update $VERSION_A -> $VERSION_B passed every check."
