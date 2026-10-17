# Updater end-to-end test, Windows part (.github/workflows/updater-test.yml; docs/TESTING.md). Runs on a
# disposable GitHub runner only: it installs Talaan, writes %APPDATA%\Talaan and changes a registry ACL.
# Never run it on a PC that has a real Talaan.
#
# Installs build A silently, seeds a meeting and settings (Open at login on), starts A with TALAAN_UPDATER_E2E=install
# (the updater-e2e hook runs the same update check and install the Settings buttons run), and checks that the
# installer replaced A with B by itself, B reopened by itself, and the meeting, settings and Open at login survived.
# Every check runs; the script fails at the end if any did. Logs and reports go to -Out.
param(
  [Parameter(Mandatory)] [string]$InstallerA,
  [Parameter(Mandatory)] [string]$VersionA,
  [Parameter(Mandatory)] [string]$VersionB,
  [Parameter(Mandatory)] [string]$FeedRoot,
  [Parameter(Mandatory)] [int]$Port,
  [Parameter(Mandatory)] [string]$Out
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..')
New-Item -ItemType Directory -Force $Out | Out-Null
$failures = [System.Collections.Generic.List[string]]::new()
function Fail([string]$m) { Write-Host "::error::$m"; $failures.Add($m) }
function Pass([string]$m) { Write-Host "ok: $m" }
function Check([bool]$ok, [string]$m) { if ($ok) { Pass $m } else { Fail $m } }

$data = Join-Path $env:APPDATA 'Talaan'
$logs = Join-Path $env:LOCALAPPDATA 'com.craftconceptsdigital.talaan\logs'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$hklmRun = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
$meetingId = '2026-10-01_0900_upd1'
$meetingDir = Join-Path $data "meetings\$meetingId"

function Get-RunValue([string]$key) { (Get-ItemProperty -Path $key -Name 'Talaan' -ErrorAction SilentlyContinue).Talaan }
function Get-Report([string]$version) {
  $f = Join-Path $logs "updater-e2e-$version.json"
  if (Test-Path $f) { try { return Get-Content $f -Raw | ConvertFrom-Json } catch { return $null } }
  return $null
}
function Wait-For([scriptblock]$cond, [int]$seconds, [scriptblock]$each = {}) {
  $until = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $until) {
    & $each
    $v = & $cond
    if ($v) { return $v }
    Start-Sleep -Milliseconds 500
  }
  return $null
}
function Get-Installed {
  Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
    Where-Object { $_.DisplayName -eq 'Talaan' } | Select-Object -First 1
}
function Save-Logs {
  if (Test-Path $logs) { Copy-Item $logs (Join-Path $Out 'app-logs') -Recurse -Force }
  if (Test-Path (Join-Path $data 'settings.json')) { Copy-Item (Join-Path $data 'settings.json') (Join-Path $Out 'settings.after.json') -Force }
  reg export 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run' (Join-Path $Out 'hkcu-run.reg') /y | Out-Null
  reg export 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' (Join-Path $Out 'hkcu-startupapproved-run.reg') /y 2>$null | Out-Null
  Get-Process | Where-Object { $_.ProcessName -match 'talaan|setup' } | Select-Object Id, ProcessName, Path | Format-Table -AutoSize | Out-String | Set-Content (Join-Path $Out 'processes.txt')
}

# 1. Act like a normal user. The runner is an elevated administrator, so auto-launch (Dynamic mode) would write
#    HKLM\...\Run; users get HKCU\...\Run, which is also what the installer's update and uninstall paths touch.
$acl = Get-Acl $hklmRun
$me = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl.AddAccessRule([Security.AccessControl.RegistryAccessRule]::new($me, 'SetValue,CreateSubKey', 'Deny'))
Set-Acl $hklmRun $acl
Write-Host "HKLM Run is read-only for $me (like a normal user)"

# 2. Install A silently.
$p = Start-Process -FilePath $InstallerA -ArgumentList '/S' -Wait -PassThru
Check ($p.ExitCode -eq 0) "installer A exited with $($p.ExitCode)"
$installed = Get-Installed
if (-not $installed) { Fail 'Talaan is not in HKCU Uninstall after installing A'; Save-Logs; exit 1 }
$dir = "$($installed.InstallLocation)".Trim('"')
if (-not $dir) { $dir = Join-Path $env:LOCALAPPDATA 'Talaan' }
$exe = Join-Path $dir 'talaan.exe'
Write-Host "installed to $dir"
Check (Test-Path $exe) "talaan.exe is in $dir"
Check ("$((Get-Item $exe).VersionInfo.ProductVersion)" -like "$VersionA*") "talaan.exe is $VersionA before the update ($((Get-Item $exe).VersionInfo.ProductVersion))"

# 3. Seed a meeting and settings, the way an existing user has them.
New-Item -ItemType Directory -Force $meetingDir | Out-Null
$meeting = [ordered]@{
  schemaVersion = 1; id = $meetingId; title = 'Updater test meeting'; app = 'zoom'; appLabel = 'Zoom'
  startedAt = '2026-10-01T09:00:00-07:00'; durationMs = 60000; language = 'en'; status = 'ready'; pinned = $true
}
[IO.File]::WriteAllText((Join-Path $meetingDir 'meeting.json'), ($meeting | ConvertTo-Json))
[IO.File]::WriteAllText((Join-Path $meetingDir 'transcript.md'), "# Updater test meeting`n`nMe: this file must survive the update.`n")
$settings = [ordered]@{ onboardingDone = $true; yourName = 'Updater Test'; launchAtLogin = $true; checkUpdates = $true; theme = 'dark' }
[IO.File]::WriteAllText((Join-Path $data 'settings.json'), ($settings | ConvertTo-Json))
$transcriptHash = (Get-FileHash (Join-Path $meetingDir 'transcript.md')).Hash
Check (-not (Get-RunValue $runKey)) 'no Open at login entry before A starts'

# 4. The feed: latest.json and B's installer on 127.0.0.1.
$serverLog = Join-Path $Out 'feed-server.log'
$server = Start-Process -FilePath node -ArgumentList 'scripts/updater-e2e/serve.mjs', "`"$FeedRoot`"", $Port -PassThru -NoNewWindow -RedirectStandardOutput $serverLog -RedirectStandardError (Join-Path $Out 'feed-server.err.log')
$up = Wait-For { try { (Invoke-WebRequest "http://127.0.0.1:$Port/" -UseBasicParsing -SkipHttpErrorCheck -TimeoutSec 2).StatusCode } catch { $null } } 20
Check ([bool]$up) "feed server is up on $Port"

# 5. Start A; its hook checks, downloads, verifies and runs the installer, which replaces A and reopens Talaan.
$env:TALAAN_UPDATER_E2E = 'install'
Start-Process -FilePath $exe | Out-Null
Remove-Item Env:\TALAAN_UPDATER_E2E
$runSeen = [System.Collections.Generic.List[string]]::new()
$watchRun = {
  $state = if (Get-RunValue $runKey) { 'present' } else { 'absent' }
  if ($runSeen.Count -eq 0 -or $runSeen[$runSeen.Count - 1] -ne $state) { $runSeen.Add($state); Write-Host "Open at login entry: $state ($(Get-Date -Format HH:mm:ss.fff))" }
}
$a = Wait-For { Get-Report $VersionA } 180 $watchRun
if (-not $a) { Fail "A wrote no updater-e2e-$VersionA.json within 3 minutes"; Save-Logs; exit 1 }
Check ($a.meetings -contains $meetingId) 'A lists the seeded meeting'
Check ($a.autostart -eq $true) "A turned Open at login on (autostart: $($a.autostart))"
$b = Wait-For { Get-Report $VersionB } 420 $watchRun
$a = Get-Report $VersionA
($a | ConvertTo-Json -Depth 6) | Set-Content (Join-Path $Out "report-$VersionA.json")
Check ($a.check.ok -eq $true -and $a.check.found.version -eq $VersionB) "A found $VersionB (check: $($a.check | ConvertTo-Json -Compress -Depth 4))"
Check ($null -eq $a.install.error) "A's install didn't fail ($($a.install | ConvertTo-Json -Compress))"
Check ([bool]$b) "Talaan reopened by itself as $VersionB after the update"
if (-not $b) {
  # Finish the other checks anyway: start B by hand.
  if ("$((Get-Item $exe).VersionInfo.ProductVersion)" -like "$VersionB*") {
    Start-Process -FilePath $exe | Out-Null
    $b = Wait-For { Get-Report $VersionB } 120 $watchRun
  }
}
Write-Host "Open at login entry over the update: $($runSeen -join ' -> ')"
($runSeen -join "`n") | Set-Content (Join-Path $Out 'run-entry-states.txt')

# 6. After the update.
$ver = "$((Get-Item $exe).VersionInfo.ProductVersion)"
Check ($ver -like "$VersionB*") "talaan.exe is $VersionB after the update ($ver)"
$installed = Get-Installed
Check ("$($installed.DisplayVersion)" -eq $VersionB) "Apps list says $VersionB ($($installed.DisplayVersion))"
if ($b) {
  ($b | ConvertTo-Json -Depth 6) | Set-Content (Join-Path $Out "report-$VersionB.json")
  Check ($b.version -eq $VersionB) "B reports version $($b.version)"
  Check ($b.meetings -contains $meetingId) 'B lists the seeded meeting'
  Check ($b.settings.launchAtLogin -eq $true) 'B reads Open at login = on from settings'
  Check ($b.settings.yourName -eq 'Updater Test') "B reads the seeded name ($($b.settings.yourName))"
  Check ($b.autostart -eq $true) "B says Open at login is on (autostart: $($b.autostart))"
}
Check (Test-Path (Join-Path $meetingDir 'meeting.json')) 'meeting.json is still there'
Check ((Test-Path (Join-Path $meetingDir 'transcript.md')) -and (Get-FileHash (Join-Path $meetingDir 'transcript.md')).Hash -eq $transcriptHash) 'transcript.md is unchanged'
$s = Get-Content (Join-Path $data 'settings.json') -Raw | ConvertFrom-Json
Check ($s.launchAtLogin -eq $true -and $s.yourName -eq 'Updater Test') 'settings.json still has the seeded values'
$run = Get-RunValue $runKey
Check ($run -and $run -like "*$exe*") "HKCU Run 'Talaan' points at the installed exe ($run)"
Check (-not (Get-RunValue $hklmRun)) 'nothing in HKLM Run'
$feedLog = if (Test-Path $serverLog) { Get-Content $serverLog -Raw } else { '' }
Check ($feedLog -match "GET \S*_x64-setup\.exe 200") "B's installer was downloaded from the feed"

# 7. Clean up and report.
Get-Process -Name talaan, talaan-llm -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue }
Save-Logs
if ($failures.Count -gt 0) {
  Write-Host "`n$($failures.Count) check(s) failed:"; $failures | ForEach-Object { Write-Host "  - $_" }
  exit 1
}
Write-Host "`nUpdate $VersionA -> $VersionB passed every check."
exit 0
