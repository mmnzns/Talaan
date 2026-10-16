# Windows release builds: whisper.cpp and llama.cpp (ggml) for a fixed x86-64-v3 CPU baseline (AVX, AVX2, FMA, F16C,
# BMI2; every PC since about 2015), never for the build machine. whisper.cpp defaults to GGML_NATIVE=ON, which
# compiles for whatever CPU builds it: on a CI runner (or a Zen 4 PC) with AVX-512 the shipped talaan.exe would stop
# with "illegal instruction" on most Intel desktops and laptops the first time Whisper runs. whisper-rs-sys and
# llama-cpp-sys-2 both forward GGML_* environment variables as CMake options (llama-cpp-sys-2 already forces
# NATIVE off itself). Dot-source this before cargo builds. Windows only: the Mac builds natively on Apple silicon,
# where these x86 options don't apply, and is left exactly as it was.
$env:GGML_NATIVE = 'OFF'
foreach ($on in 'GGML_SSE42', 'GGML_AVX', 'GGML_AVX2', 'GGML_BMI2', 'GGML_FMA', 'GGML_F16C') { Set-Item "env:$on" 'ON' }
foreach ($off in 'GGML_AVX_VNNI', 'GGML_AVX512', 'GGML_AVX512_VBMI', 'GGML_AVX512_VNNI', 'GGML_AVX512_BF16') { Set-Item "env:$off" 'OFF' }

# The CMake caches of whisper.cpp and llama.cpp under a target folder (release profile).
function Get-GgmlCaches([string]$target) {
  $build = Join-Path $target 'release\build'
  if (-not (Test-Path $build)) { return @() }
  Get-ChildItem $build -Directory |
    Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
    ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
}

# Problems with a cache: built for the build machine's CPU, with AVX-512, or without AVX2.
function Test-GgmlBaseline([string]$cache) {
  $text = Get-Content $cache
  $bad = @($text | Where-Object { $_ -match '^GGML_NATIVE:BOOL=ON' -or $_ -match '^GGML_AVX512[A-Z0-9_]*:BOOL=ON' })
  if (-not ($text | Where-Object { $_ -match '^GGML_AVX2:BOOL=ON' })) { $bad += 'GGML_AVX2 is not ON' }
  $bad
}

# Cargo doesn't rebuild the -sys crates when only the environment changed, so a cache made for this machine's CPU
# would be reused: clean those crates first.
function Reset-StaleGgml([string]$manifest, [string]$target) {
  $stale = @(Get-GgmlCaches $target | Where-Object { @(Test-GgmlBaseline $_).Count -gt 0 })
  if ($stale.Count -gt 0) {
    Write-Host "Rebuilding whisper.cpp and llama.cpp for the x86-64-v3 baseline"
    cargo clean --manifest-path $manifest --target-dir $target --release -p whisper-rs-sys -p llama-cpp-sys-2
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  }
}

# Ninja, not the Visual Studio generator, for the Vulkan builds (as llama.cpp's own Windows Vulkan CI does): under
# MSBuild the vulkan-shaders-gen ExternalProject's install step failed ("Not a file: .../cmake_install.cmake",
# "cannot find the batch label specified - VCEnd"). cmake-rs passes cl.exe and its environment to a Ninja build.
# Only these builds: talaan.exe and the CPU helper keep the generator they were released with. Call it after the
# CPU builds (it sets CMAKE_GENERATOR for the rest of the session).
function Use-NinjaForGpu {
  if (-not (Get-Command ninja -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $ninja = if (Test-Path $vswhere) { & $vswhere -latest -products * -find 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe' | Select-Object -First 1 }
    if (-not $ninja) { throw "ninja.exe not found (install Visual Studio's C++ CMake tools, or Ninja)" }
    $env:PATH = "$(Split-Path $ninja);$env:PATH"
  }
  $env:CMAKE_GENERATOR = 'Ninja'
}
