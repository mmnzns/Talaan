# Builds talaan-llm (the local AI helper, its own process so its llama.cpp never meets Whisper's ggml) and puts
# it where Tauri's externalBin expects it: src-tauri/binaries/talaan-llm-<target triple>.exe. Always optimized:
# llama.cpp is unusably slow in a debug build. Also copied next to the dev binaries so `tauri dev` finds it.
#
# GPU helpers (docs/plans/G1-windows-gpu.md): when VULKAN_SDK is set, also builds talaan-llm-vulkan.exe and
# talaan-stt-vulkan.exe into src-tauri/binaries (the release adds them through bundle.resources) and next to the dev
# binaries. They build in their own target folder (TALAAN_GPU_TARGET_DIR, default src-tauri\target\gpu; keep it
# short on CI, whisper.cpp's Vulkan build hits path-length limits) so a Vulkan ggml never mixes with the CPU one.
# TALAAN_GPU=1 makes them required: the build fails without the Vulkan SDK instead of quietly skipping them.
$ErrorActionPreference = 'Stop'
# llama.cpp and whisper.cpp for a fixed CPU baseline, not this machine's (scripts/cpu-baseline.ps1). The GPU
# helpers' CPU code (and the processor fallback inside them) follows the same baseline.
. (Join-Path $PSScriptRoot 'cpu-baseline.ps1')
Set-Location (Join-Path $PSScriptRoot '..\src-tauri')
$triple = ((rustc -vV) | Where-Object { $_ -like 'host: *' }).Substring(6)
cargo build --release -p talaan-llm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force binaries, target\debug | Out-Null
Copy-Item target\release\talaan-llm.exe "binaries\talaan-llm-$triple.exe" -Force
try { Copy-Item target\release\talaan-llm.exe target\debug\talaan-llm.exe -Force -ErrorAction Stop } catch { Write-Warning "Could not update target\debug\talaan-llm.exe (is tauri dev running?): $_" }

if (-not $env:VULKAN_SDK) {
  if ($env:TALAAN_GPU -eq '1') { throw "TALAAN_GPU=1 needs the Vulkan SDK, and VULKAN_SDK is not set" }
  Write-Host "VULKAN_SDK is not set: no GPU helpers (Talaan uses the processor)"
  exit 0
}

$gpuTarget = if ($env:TALAAN_GPU_TARGET_DIR) { $env:TALAAN_GPU_TARGET_DIR } else { 'target\gpu' }
Reset-StaleGgml 'Cargo.toml' $gpuTarget

# Ninja for the Vulkan builds (scripts/cpu-baseline.ps1, Use-NinjaForGpu).
Use-NinjaForGpu
# A cache from another generator can't be reused: CMake refuses to switch generators in a build folder.
$otherGenerator = @(Get-GgmlCaches $gpuTarget | Where-Object { -not (Select-String -Path $_ -Pattern '^CMAKE_GENERATOR:[A-Z]+=Ninja$' -Quiet) })
if ($otherGenerator.Count -gt 0) {
  Write-Host "Rebuilding the GPU helpers' whisper.cpp and llama.cpp with Ninja"
  cargo clean --manifest-path Cargo.toml --target-dir $gpuTarget --release -p whisper-rs-sys -p llama-cpp-sys-2
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
foreach ($crate in 'talaan-llm', 'talaan-stt') {
  cargo build --release -p $crate --features vulkan --target-dir $gpuTarget
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  $built = Join-Path $gpuTarget "release\$crate.exe"
  Copy-Item $built "binaries\$crate-vulkan.exe" -Force
  try { Copy-Item $built "target\debug\$crate-vulkan.exe" -Force -ErrorAction Stop } catch { Write-Warning "Could not update target\debug\$crate-vulkan.exe (is tauri dev running?): $_" }
}
Write-Host "GPU helpers: binaries\talaan-llm-vulkan.exe, binaries\talaan-stt-vulkan.exe"
