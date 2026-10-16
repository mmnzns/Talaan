# Talks to the GPU helpers directly, the way Talaan does, without the app (docs/plans/G1-windows-gpu.md).
# Run from any folder; nothing is written except a temp file per request.
#
#   powershell -File scripts\gpu-helper-smoke.ps1 -Dir <folder with talaan-llm-vulkan.exe and talaan-stt-vulkan.exe>
#       [-LlmModel <.gguf>]      a short summary-style completion on the GPU
#       [-WhisperModel <.bin> -Wav <16 kHz mono 16-bit .wav>]   the first 30 s transcribed on the GPU
#
# Each helper starts with implicit Vulkan layers off, like Talaan starts it. Without a model it only checks that the
# helper starts and shows the devices it sees and the one it chose. Set TALAAN_GPU=fail to see a GPU build stop on
# purpose (the app then falls back to the processor), or GGML_VK_VISIBLE_DEVICES=99 to make Vulkan fail to start.
param(
  [Parameter(Mandatory = $true)][string]$Dir,
  [string]$LlmModel,
  [string]$WhisperModel,
  [string]$Wav
)
$ErrorActionPreference = 'Stop'

function Invoke-Helper([string]$exe, [string[]]$requests) {
  if (-not (Test-Path $exe)) { throw "Missing $exe" }
  $in = [IO.Path]::GetTempFileName()
  [IO.File]::WriteAllLines($in, $requests, (New-Object Text.UTF8Encoding $false))
  $env:VK_LOADER_LAYERS_DISABLE = '~implicit~'
  $started = Get-Date
  # cmd's redirection hands the file over byte for byte (a PowerShell pipe would re-encode it).
  $out = cmd /c "`"$exe`" < `"$in`""
  $code = $LASTEXITCODE
  Remove-Item $in
  Write-Host ("  {0:N1} s, exit code {1}" -f ((Get-Date) - $started).TotalSeconds, $code)
  foreach ($line in $out) {
    $short = if ($line.Length -gt 400) { $line.Substring(0, 400) + '...' } else { $line }
    Write-Host "  $short"
  }
  if (-not ($out | Where-Object { $_ -like '*"event":"ready"*' })) { throw "$exe didn't say it was ready" }
  $out
}

function ConvertTo-Json1($o) { $o | ConvertTo-Json -Compress -Depth 5 }

$llm = Join-Path $Dir 'talaan-llm-vulkan.exe'
$stt = Join-Path $Dir 'talaan-stt-vulkan.exe'

Write-Host "talaan-llm-vulkan: ping, devices"
$reqs = @('{"op":"ping"}', '{"op":"devices"}')
if ($LlmModel) {
  $reqs += ConvertTo-Json1 @{ op = 'complete'; model_path = (Resolve-Path $LlmModel).Path; n_ctx = 16384; system = 'Reply with JSON only.'; user = "Give a title and a one-sentence recap for: 'We agreed to ship onboarding first and move pricing to Q1.'"; json_schema = '{"type":"object","properties":{"title":{"type":"string"},"recap":{"type":"string"}},"required":["title","recap"]}'; max_tokens = 200; temperature = 0.0; gpu = $true }
  Write-Host "  ...and a completion with $LlmModel"
}
$null = Invoke-Helper $llm $reqs

Write-Host "talaan-stt-vulkan: ping, devices"
$reqs = @('{"op":"ping"}', '{"op":"devices"}')
if ($WhisperModel) {
  $reqs += ConvertTo-Json1 @{ op = 'load'; model_path = (Resolve-Path $WhisperModel).Path; gpu = $true }
  if ($Wav) {
    # 16-bit PCM after the 44-byte header, at most 30 s, as little-endian f32 in base64 (stt protocol).
    $bytes = [IO.File]::ReadAllBytes((Resolve-Path $Wav).Path)
    $n = [Math]::Min(($bytes.Length - 44) / 2, 16000 * 30)
    $f = New-Object byte[] ($n * 4)
    for ($i = 0; $i -lt $n; $i++) {
      $s = [BitConverter]::ToInt16($bytes, 44 + 2 * $i) / 32768.0
      [BitConverter]::GetBytes([single]$s).CopyTo($f, 4 * $i)
    }
    $reqs += ConvertTo-Json1 @{ op = 'transcribe'; samples = [Convert]::ToBase64String($f) }
    Write-Host ("  ...and {0:N1} s of {1}" -f ($n / 16000), $Wav)
  }
}
$null = Invoke-Helper $stt $reqs
Write-Host "Both helpers started."
