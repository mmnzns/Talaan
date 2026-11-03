# Dev helper (Windows): builds the helpers Talaan spawns next to talaan.exe and puts them where Tauri's externalBin
# expects them: src-tauri\binaries\<name>-<target triple>.exe (see tauri.windows.conf.json). Always optimized:
# llama.cpp and whisper.cpp are unusably slow in a debug build. Also copied next to the dev binaries (target\debug)
# so `tauri dev` finds them.
#
#   talaan-llm, talaan-stt              the processor helpers, always built.
#   talaan-llm-vulkan, talaan-stt-vulkan  built with --features vulkan only when VULKAN_SDK is set, in their own
#                                       target folder (TALAAN_GPU_TARGET_DIR, default src-tauri\target\gpu; keep it
#                                       short, whisper.cpp's Vulkan build hits path-length limits) so a Vulkan ggml
#                                       never mixes with the CPU one.
# Without the Vulkan SDK the processor builds are copied under the -vulkan names too, so Tauri's externalBin check
# passes. That is for dev only: the copies never touch a GPU. Releases use the helpers the engine's build-helpers.yml
# builds (release.yml), and set TALAAN_HELPERS_PREBUILT=1, which makes this script do nothing.
# The CPU baseline (GGML_*, -O2) comes from the root .cargo/config.toml.
$ErrorActionPreference = 'Stop'
if ($env:TALAAN_HELPERS_PREBUILT -eq '1') {
  Write-Host "TALAAN_HELPERS_PREBUILT=1: using the prebuilt helpers in src-tauri\binaries"
  exit 0
}
Set-Location (Join-Path $PSScriptRoot '..\src-tauri')
$triple = ((rustc -vV) | Where-Object { $_ -like 'host: *' }).Substring(6)
New-Item -ItemType Directory -Force binaries, target\debug | Out-Null

function Install-Helper([string]$built, [string]$name) {
  Copy-Item $built "binaries\$name-$triple.exe" -Force
  try { Copy-Item $built "target\debug\$name.exe" -Force -ErrorAction Stop } catch { Write-Warning "Could not update target\debug\$name.exe (is tauri dev running?): $_" }
}

foreach ($crate in 'talaan-llm', 'talaan-stt') {
  cargo build --release -p $crate
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  Install-Helper "target\release\$crate.exe" $crate
}

if (-not $env:VULKAN_SDK) {
  Write-Host "VULKAN_SDK is not set: the -vulkan helpers are copies of the processor builds (dev only)"
  foreach ($crate in 'talaan-llm', 'talaan-stt') { Install-Helper "target\release\$crate.exe" "$crate-vulkan" }
  exit 0
}

$gpuTarget = if ($env:TALAAN_GPU_TARGET_DIR) { $env:TALAAN_GPU_TARGET_DIR } else { 'target\gpu' }
# Ninja, not the Visual Studio generator, for the Vulkan builds (as llama.cpp's own Windows Vulkan CI does): under
# MSBuild the vulkan-shaders-gen ExternalProject's install step failed. cmake-rs passes cl.exe and its environment to
# a Ninja build. Only these builds: the processor helpers keep the generator they were built with.
if (-not (Get-Command ninja -ErrorAction SilentlyContinue)) {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  $ninja = if (Test-Path $vswhere) { & $vswhere -latest -products * -find 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe' | Select-Object -First 1 }
  if (-not $ninja) { throw "ninja.exe not found (install Visual Studio's C++ CMake tools, or Ninja)" }
  $env:PATH = "$(Split-Path $ninja);$env:PATH"
}
$env:CMAKE_GENERATOR = 'Ninja'
# A cache from another generator can't be reused: CMake refuses to switch generators in a build folder.
$gpuBuild = Join-Path $gpuTarget 'release\build'
$otherGenerator = @()
if (Test-Path $gpuBuild) {
  $otherGenerator = @(Get-ChildItem $gpuBuild -Directory |
    Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
    ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } |
    Where-Object { (Test-Path $_) -and -not (Select-String -Path $_ -Pattern '^CMAKE_GENERATOR:[A-Z]+=Ninja$' -Quiet) })
}
if ($otherGenerator.Count -gt 0) {
  Write-Host "Rebuilding the GPU helpers' whisper.cpp and llama.cpp with Ninja"
  cargo clean --target-dir $gpuTarget --release -p whisper-rs-sys -p llama-cpp-sys-2
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
foreach ($crate in 'talaan-llm', 'talaan-stt') {
  cargo build --release -p $crate --features vulkan --target-dir $gpuTarget
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  Install-Helper (Join-Path $gpuTarget "release\$crate.exe") "$crate-vulkan"
}
Write-Host "GPU helpers: binaries\talaan-llm-vulkan-$triple.exe, binaries\talaan-stt-vulkan-$triple.exe"
