#!/bin/bash
# Builds talaan-llm (the local AI helper, its own process so its llama.cpp never meets Whisper's ggml) and puts
# it where Tauri's externalBin expects it: src-tauri/binaries/talaan-llm-<target triple>. Always optimized:
# llama.cpp is unusably slow in a debug build. Also copied next to the dev binaries so `tauri dev` finds it.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
cargo build --release -p talaan-llm
mkdir -p binaries target/debug
cp target/release/talaan-llm "binaries/talaan-llm-$TRIPLE"
cp target/release/talaan-llm target/debug/talaan-llm
