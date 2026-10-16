# Builds the Windows release: Talaan_<version>_x64-setup.exe and, when TAURI_SIGNING_PRIVATE_KEY is set (CI),
# its updater signature (.exe.sig). The release workflow runs this on windows-latest; run it on the PC to check
# the same build before tagging. Output: src-tauri\target\release\bundle\nsis.
#
# GPU (docs/plans/G1-windows-gpu.md): with the Vulkan SDK (VULKAN_SDK set; TALAAN_GPU=1 makes it required) the
# installer also carries talaan-llm-vulkan.exe and talaan-stt-vulkan.exe next to talaan.exe. talaan.exe itself
# never links Vulkan: the script fails if it (or the CPU talaan-llm.exe) imports vulkan-1.dll.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$version = node -p "require('./src-tauri/tauri.conf.json').version"
if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne "v$version") { throw "Tag $env:GITHUB_REF_NAME but tauri.conf.json says $version" }

# whisper.cpp and llama.cpp for a fixed CPU baseline, not this machine's (scripts/cpu-baseline.ps1).
. (Join-Path $PSScriptRoot 'cpu-baseline.ps1')
Reset-StaleGgml 'src-tauri/Cargo.toml' 'src-tauri/target'

$gpu = [bool]$env:VULKAN_SDK
if ($env:TALAAN_GPU -eq '1' -and -not $gpu) { throw "TALAAN_GPU=1 needs the Vulkan SDK, and VULKAN_SDK is not set" }
# The helpers, CPU and GPU (tauri build runs this again as beforeBuildCommand; by then it's a no-op).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# ONNX Runtime (Parakeet, diarization) is linked statically but imports DirectML.dll. ort-sys's build script
# drops it next to the binaries (as a link into its cache); copy the real file where the bundle can take it.
cargo build --release --manifest-path src-tauri/Cargo.toml -p ort-sys
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force src-tauri\binaries | Out-Null
Copy-Item src-tauri\target\release\DirectML.dll src-tauri\binaries\DirectML.dll -Force

# Release-only config: updater artifacts when the key is there, DirectML.dll beside talaan.exe, and the GPU helpers.
$signed = [bool]$env:TAURI_SIGNING_PRIVATE_KEY
$resources = @{ 'binaries/DirectML.dll' = 'DirectML.dll' }
if ($gpu) {
  foreach ($h in 'talaan-llm-vulkan.exe', 'talaan-stt-vulkan.exe') {
    if (-not (Test-Path "src-tauri\binaries\$h")) { throw "GPU helper missing: src-tauri\binaries\$h" }
    $resources["binaries/$h"] = $h
  }
}
$conf = @{ bundle = @{ createUpdaterArtifacts = $signed; resources = $resources } } | ConvertTo-Json -Depth 5
$confPath = Join-Path $env:TEMP 'talaan-release.conf.json'
[IO.File]::WriteAllText($confPath, $conf)
npm run tauri build -- --config $confPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# whisper.cpp and llama.cpp must be optimized (.cargo/config.toml); cmake-rs alone builds them at /Od.
function Test-Optimized($caches) {
  foreach ($c in $caches) {
    $lines = Select-String -Path $c -Pattern '^CMAKE_(C|CXX)_FLAGS_RELEASE:STRING=(.*)$'
    foreach ($l in $lines) { if ($l.Matches[0].Groups[2].Value -notmatch '-O2') { throw "Not optimized: $c ($($l.Line))" } }
  }
}
# ...and for the baseline CPU: an AVX-512 or native build crashes on most Intel PCs.
function Test-Baseline($caches) {
  foreach ($c in $caches) {
    $bad = @(Test-GgmlBaseline $c)
    if ($bad.Count -gt 0) { throw "Not built for the x86-64-v3 baseline: $c ($($bad -join '; '))" }
  }
}
function Test-BothBuilt($caches, [string]$what) {
  if (@($caches | Where-Object { $_ -match 'whisper-rs-sys' }).Count -eq 0 -or @($caches | Where-Object { $_ -match 'llama-cpp-sys-2' }).Count -eq 0) { throw "No CMake cache for $what whisper.cpp or llama.cpp" }
}
function Test-Vulkan($caches, [bool]$on) {
  foreach ($c in $caches) {
    $vk = [bool](Select-String -Path $c -Pattern '^GGML_VULKAN:BOOL=ON' -Quiet)
    if ($vk -ne $on) { throw "GGML_VULKAN should be $(if ($on) { 'ON' } else { 'OFF' }): $c" }
  }
}

$caches = @(Get-GgmlCaches 'src-tauri\target')
Test-BothBuilt $caches 'the'
Test-Optimized $caches
Test-Baseline $caches
Test-Vulkan $caches $false
Write-Host "whisper.cpp and llama.cpp built with -O2 for x86-64-v3 (AVX2, no AVX-512, not native), without Vulkan"

if ($gpu) {
  $gpuTarget = if ($env:TALAAN_GPU_TARGET_DIR) { $env:TALAAN_GPU_TARGET_DIR } else { 'src-tauri\target\gpu' }
  $gpuCaches = @(Get-GgmlCaches $gpuTarget)
  Test-BothBuilt $gpuCaches 'the GPU helpers'
  Test-Optimized $gpuCaches
  Test-Baseline $gpuCaches
  Test-Vulkan $gpuCaches $true
  Write-Host "GPU helpers: whisper.cpp and llama.cpp built with Vulkan, -O2 and the x86-64-v3 baseline for their CPU code"
}

# talaan.exe must start on a PC without a GPU driver: it may never import vulkan-1.dll (only the helpers do).
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = (Get-Command dumpbin.exe -ErrorAction SilentlyContinue).Source
if (-not $dumpbin -and (Test-Path $vswhere)) {
  $dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
}
if (-not $dumpbin) { throw "dumpbin.exe not found (Visual Studio C++ tools); it checks that talaan.exe doesn't import vulkan-1.dll" }
function Get-Imports([string]$file) {
  if (-not (Test-Path $file)) { throw "Missing $file" }
  & $dumpbin /nologo /dependents $file | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '\.dll$' }
}
foreach ($f in 'src-tauri\target\release\talaan.exe', 'src-tauri\target\release\talaan-llm.exe') {
  if (@(Get-Imports $f) -contains 'vulkan-1.dll') { throw "$f imports vulkan-1.dll; only the GPU helpers may" }
}
Write-Host "talaan.exe and talaan-llm.exe don't import vulkan-1.dll"
if ($gpu) {
  foreach ($h in 'talaan-llm-vulkan.exe', 'talaan-stt-vulkan.exe') {
    if (-not (@(Get-Imports "src-tauri\binaries\$h") -contains 'vulkan-1.dll')) { throw "$h doesn't import vulkan-1.dll: not a Vulkan build" }
  }
  Write-Host "talaan-llm-vulkan.exe and talaan-stt-vulkan.exe are Vulkan builds"
}

$out = 'src-tauri\target\release\bundle\nsis'
$exe = Get-ChildItem $out -Filter '*_x64-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $exe) { throw "No installer in $out" }
if ($signed -and -not (Test-Path "$($exe.FullName).sig")) { throw "No updater signature for $($exe.Name)" }
Get-ChildItem $out -Filter "$($exe.BaseName)*" | ForEach-Object { "{0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
