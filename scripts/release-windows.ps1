# Builds the Windows release: Talaan_<version>_x64-setup.exe and, when TAURI_SIGNING_PRIVATE_KEY is set (CI),
# its updater signature (.exe.sig). The release workflow runs this on windows-latest; run it on the PC to check
# the same build before tagging. Output: src-tauri\target\release\bundle\nsis.

# Helpers: Talaan spawns talaan-llm, talaan-stt, talaan-llm-vulkan and talaan-stt-vulkan next to talaan.exe; Tauri
# bundles them as sidecars (tauri.windows.conf.json externalBin, src-tauri\binaries\<name>-<triple>.exe). The release
# workflow downloads them from the engine's build-helpers.yml and sets TALAAN_HELPERS_PREBUILT=1; this script then
# builds none. Without it (a local check on the PC) scripts/build-llm-helper.ps1 builds them, and without the Vulkan
# SDK its -vulkan files are processor copies (dev only), so the Vulkan checks below are skipped.
# talaan.exe never links Vulkan: the script fails if it (or a processor helper) imports vulkan-1.dll.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$version = node -p "require('./src-tauri/tauri.conf.json').version"
if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne "v$version") { throw "Tag $env:GITHUB_REF_NAME but tauri.conf.json says $version" }
$triple = ((rustc -vV) | Where-Object { $_ -like 'host: *' }).Substring(6)
$prebuilt = $env:TALAAN_HELPERS_PREBUILT -eq '1'

# The app's own in-process whisper.cpp (voice-engine-stt) is built for the CPU baseline in the root .cargo/config.toml
# (GGML_* in [env]); these read it back from the CMake caches. A cache made before that file applied (native CPU,
# AVX-512, no optimization) is not rebuilt by cargo on its own, so it is cleaned first.
$cargoConfig = Get-Content (Join-Path $PWD '.cargo\config.toml')
$expected = @{}
foreach ($l in $cargoConfig) { if ($l -match '^(GGML_[A-Z0-9_]+)\s*=\s*\{\s*value\s*=\s*"(ON|OFF)"') { $expected[$Matches[1]] = $Matches[2] } }
if (-not $expected.ContainsKey('GGML_NATIVE') -or -not $expected.ContainsKey('GGML_AVX2')) { throw ".cargo/config.toml has no GGML_NATIVE / GGML_AVX2 settings" }

# The CMake caches of whisper.cpp and llama.cpp under a target folder (release profile).
function Get-GgmlCaches([string]$target) {
  $build = Join-Path $target 'release\build'
  if (-not (Test-Path $build)) { return @() }
  Get-ChildItem $build -Directory |
    Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
    ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
}
# Problems with a cache: a GGML_* option that differs from .cargo/config.toml, AVX-512 on, or no AVX2 entry at all.
function Test-GgmlBaseline([string]$cache) {
  $text = Get-Content $cache
  $bad = @()
  foreach ($k in $expected.Keys) {
    $m = @($text | Where-Object { $_ -match "^$($k):BOOL=(ON|OFF)" })
    if ($m.Count -gt 0 -and $m[0] -notmatch "BOOL=$($expected[$k])$") { $bad += "$k is not $($expected[$k])" }
  }
  if ($text | Where-Object { $_ -match '^GGML_AVX512[A-Z0-9_]*:BOOL=ON' }) { $bad += 'AVX-512 is on' }
  if (-not ($text | Where-Object { $_ -match '^GGML_AVX2:BOOL=' })) { $bad += 'GGML_AVX2 is missing' }
  $bad
}
$stale = @(Get-GgmlCaches 'src-tauri\target' | Where-Object { @(Test-GgmlBaseline $_).Count -gt 0 })
if ($stale.Count -gt 0) {
  Write-Host "Rebuilding whisper.cpp and llama.cpp for the baseline in .cargo/config.toml"
  cargo clean --manifest-path src-tauri/Cargo.toml --release -p whisper-rs-sys -p llama-cpp-sys-2
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$gpu = [bool]$env:VULKAN_SDK
if ($env:TALAAN_GPU -eq '1' -and -not $gpu -and -not $prebuilt) { throw "TALAAN_GPU=1 needs the Vulkan SDK, and VULKAN_SDK is not set" }
# The four helpers: downloaded (prebuilt) or built here. tauri build runs the dev script again as beforeBuildCommand;
# with TALAAN_HELPERS_PREBUILT=1 that is a no-op, otherwise it finds everything built.
if (-not $prebuilt) {
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
$helperNames = 'talaan-llm', 'talaan-stt', 'talaan-llm-vulkan', 'talaan-stt-vulkan'
foreach ($h in $helperNames) {
  if (-not (Test-Path "src-tauri\binaries\$h-$triple.exe")) { throw "Helper missing: src-tauri\binaries\$h-$triple.exe" }
}

# Release-only config: updater artifacts when the key is there. (DirectML.dll comes from Windows' System32; the
# helpers are externalBin entries in tauri.windows.conf.json.)
$signed = [bool]$env:TAURI_SIGNING_PRIVATE_KEY
$conf = @{ bundle = @{ createUpdaterArtifacts = $signed } } | ConvertTo-Json -Depth 5
$confPath = Join-Path $env:TEMP 'talaan-release.conf.json'
[IO.File]::WriteAllText($confPath, $conf)
# Test builds only (.github/workflows/updater-test.yml): TALAAN_TEST_CONFIG names one more config, merged last
# (scripts/updater-e2e/config.mjs: test version, throwaway updater key, local feed, the updater-e2e feature).
$testConf = @()
if ($env:TALAAN_TEST_CONFIG) {
  if ($env:GITHUB_REF_TYPE -eq 'tag') { throw "TALAAN_TEST_CONFIG is for test builds; a tag builds a release" }
  $testConf = @('--config', $env:TALAAN_TEST_CONFIG)
  Write-Host "Test build: merging $env:TALAAN_TEST_CONFIG"
}
npm run tauri build -- --config $confPath @testConf
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
function Test-WhisperBuilt($caches) {
  if (@($caches | Where-Object { $_ -match 'whisper-rs-sys' }).Count -eq 0) { throw "No CMake cache for the app's whisper.cpp" }
}
function Test-Vulkan($caches, [bool]$on) {
  foreach ($c in $caches) {
    $vk = [bool](Select-String -Path $c -Pattern '^GGML_VULKAN:BOOL=ON' -Quiet)
    if ($vk -ne $on) { throw "GGML_VULKAN should be $(if ($on) { 'ON' } else { 'OFF' }): $c" }
  }
}

$caches = @(Get-GgmlCaches 'src-tauri\target')
Test-WhisperBuilt $caches
Test-Optimized $caches
Test-Baseline $caches
Test-Vulkan $caches $false
Write-Host "The app's whisper.cpp built with -O2 and the baseline of .cargo/config.toml (AVX2, no AVX-512, not native), without Vulkan"

if ($gpu -and -not $prebuilt) {
  $gpuTarget = if ($env:TALAAN_GPU_TARGET_DIR) { $env:TALAAN_GPU_TARGET_DIR } else { 'src-tauri\target\gpu' }
  $gpuCaches = @(Get-GgmlCaches $gpuTarget)
  Test-WhisperBuilt $gpuCaches
  Test-Optimized $gpuCaches
  Test-Baseline $gpuCaches
  Test-Vulkan $gpuCaches $true
  Write-Host "GPU helpers: whisper.cpp and llama.cpp built with Vulkan, -O2 and the baseline for their CPU code"
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
foreach ($f in 'src-tauri\target\release\talaan.exe', "src-tauri\binaries\talaan-llm-$triple.exe", "src-tauri\binaries\talaan-stt-$triple.exe") {
  if (@(Get-Imports $f) -contains 'vulkan-1.dll') { throw "$f imports vulkan-1.dll; only the -vulkan helpers may" }
}
Write-Host "talaan.exe, talaan-llm.exe and talaan-stt.exe don't import vulkan-1.dll"

# The updater test's hook (src-tauri/src/commands/updater_e2e.rs, feature updater-e2e) never ships: a release
# talaan.exe must not contain its variable's name. A test build must, or the test would prove nothing.
$exeText = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes((Resolve-Path 'src-tauri\target\release\talaan.exe')))
$hasHook = $exeText.Contains('TALAAN_UPDATER_E2E')
Remove-Variable exeText
if ($env:TALAAN_TEST_CONFIG) {
  if (-not $hasHook) { throw "Test build without the updater-e2e hook: does TALAAN_TEST_CONFIG turn on the updater-e2e feature?" }
  Write-Host "Test build: talaan.exe has the updater-e2e hook (never in a release)"
} elseif ($hasHook) {
  throw "talaan.exe contains the updater-e2e test hook; release builds must not turn on the updater-e2e feature"
} else {
  Write-Host "talaan.exe has no test hook"
}
if ($gpu -or $prebuilt) {
  foreach ($h in 'talaan-llm-vulkan', 'talaan-stt-vulkan') {
    if (-not (@(Get-Imports "src-tauri\binaries\$h-$triple.exe") -contains 'vulkan-1.dll')) { throw "$h doesn't import vulkan-1.dll: not a Vulkan build" }
  }
  Write-Host "talaan-llm-vulkan.exe and talaan-stt-vulkan.exe are Vulkan builds"
} else {
  Write-Host "No Vulkan SDK and no prebuilt helpers: the -vulkan helpers are processor copies (dev only), not checked"
}

$out = 'src-tauri\target\release\bundle\nsis'
$exe = Get-ChildItem $out -Filter '*_x64-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $exe) { throw "No installer in $out" }
if ($signed -and -not (Test-Path "$($exe.FullName).sig")) { throw "No updater signature for $($exe.Name)" }
Get-ChildItem $out -Filter "$($exe.BaseName)*" | ForEach-Object { "{0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
