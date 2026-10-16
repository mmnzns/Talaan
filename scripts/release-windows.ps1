# Builds the Windows release: Talaan_<version>_x64-setup.exe and, when TAURI_SIGNING_PRIVATE_KEY is set (CI),
# its updater signature (.exe.sig). The release workflow runs this on windows-latest; run it on the PC to check
# the same build before tagging. Output: src-tauri\target\release\bundle\nsis.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$version = node -p "require('./src-tauri/tauri.conf.json').version"
if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne "v$version") { throw "Tag $env:GITHUB_REF_NAME but tauri.conf.json says $version" }

# whisper.cpp and llama.cpp for a fixed CPU baseline, not this machine's (scripts/cpu-baseline.ps1).
. (Join-Path $PSScriptRoot 'cpu-baseline.ps1')
Reset-StaleGgml 'src-tauri/Cargo.toml' 'src-tauri/target'

# ONNX Runtime (Parakeet, diarization) is linked statically but imports DirectML.dll. ort-sys's build script
# drops it next to the binaries (as a link into its cache); copy the real file where the bundle can take it.
cargo build --release --manifest-path src-tauri/Cargo.toml -p ort-sys
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force src-tauri\binaries | Out-Null
Copy-Item src-tauri\target\release\DirectML.dll src-tauri\binaries\DirectML.dll -Force

# Release-only config: updater artifacts when the key is there, and DirectML.dll beside talaan.exe.
$signed = [bool]$env:TAURI_SIGNING_PRIVATE_KEY
$conf = @{ bundle = @{ createUpdaterArtifacts = $signed; resources = @{ 'binaries/DirectML.dll' = 'DirectML.dll' } } } | ConvertTo-Json -Depth 5
$confPath = Join-Path $env:TEMP 'talaan-release.conf.json'
[IO.File]::WriteAllText($confPath, $conf)
npm run tauri build -- --config $confPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# whisper.cpp and llama.cpp must be optimized (.cargo/config.toml); cmake-rs alone builds them at /Od.
$caches = Get-ChildItem src-tauri\target\release\build -Directory |
  Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
  ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
if (@($caches | Where-Object { $_ -match 'whisper-rs-sys' }).Count -eq 0 -or @($caches | Where-Object { $_ -match 'llama-cpp-sys-2' }).Count -eq 0) { throw "No CMake cache for whisper.cpp or llama.cpp" }
foreach ($c in $caches) {
  $lines = Select-String -Path $c -Pattern '^CMAKE_(C|CXX)_FLAGS_RELEASE:STRING=(.*)$'
  foreach ($l in $lines) { if ($l.Matches[0].Groups[2].Value -notmatch '-O2') { throw "Not optimized: $c ($($l.Line))" } }
}
Write-Host "whisper.cpp and llama.cpp built with -O2"
# ...and for the baseline CPU: an AVX-512 or native build crashes on most Intel PCs.
foreach ($c in $caches) {
  $bad = @(Test-GgmlBaseline $c)
  if ($bad.Count -gt 0) { throw "Not built for the x86-64-v3 baseline: $c ($($bad -join '; '))" }
}
Write-Host "whisper.cpp and llama.cpp built for x86-64-v3 (AVX2, no AVX-512, not native)"

$out = 'src-tauri\target\release\bundle\nsis'
$exe = Get-ChildItem $out -Filter '*_x64-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $exe) { throw "No installer in $out" }
if ($signed -and -not (Test-Path "$($exe.FullName).sig")) { throw "No updater signature for $($exe.Name)" }
Get-ChildItem $out -Filter "$($exe.BaseName)*" | ForEach-Object { "{0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
