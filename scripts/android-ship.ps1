<#
.SYNOPSIS
  Build and update the existing MaxSpeech install on the phone (same package).
  Uses adb install -r only. Does not uninstall. Does not create a second app.

.EXAMPLE
  .\scripts\android-ship.ps1
  .\scripts\android-ship.ps1 -Launch
  .\scripts\android-ship.ps1 -SkipBuild
#>
param(
    [switch]$Debug,
    [switch]$SkipBuild,
    # Only reopen when you explicitly want a cold start.
    [switch]$Launch,
    [int]$WatchSeconds = 6
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$androidDir = Join-Path $root "android"
$adb = Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe"
$sdk = Join-Path $env:LOCALAPPDATA "Android\Sdk"
$jdk = "C:\Program Files\Microsoft\jdk-17.0.20.101-hotspot"

if (-not (Test-Path $adb)) { throw "adb missing: $adb" }
if (-not (Test-Path $jdk)) {
    $jdk = $env:JAVA_HOME
    if (-not $jdk -or -not (Test-Path $jdk)) { throw "JDK 17 not found" }
}

$env:ANDROID_HOME = $sdk
$env:JAVA_HOME = $jdk

$devices = & $adb devices | Select-String "`tdevice$"
if (-not $devices) {
    throw "No authorized device. Plug in phone, unlock, accept USB debugging."
}

$sdkEscaped = $sdk -replace '\\', '\\'
Set-Content -Path (Join-Path $androidDir "local.properties") -Value "sdk.dir=$sdkEscaped" -Encoding ASCII

# Always one package - debug/release share applicationId (no .debug suffix).
$variant = if ($Debug) { "Debug" } else { "Release" }
$task = ":app:assemble$variant"
$apkRel = if ($Debug) {
    "app\build\outputs\apk\debug\app-debug.apk"
} else {
    "app\build\outputs\apk\release\app-release.apk"
}
$apk = Join-Path $androidDir $apkRel
$pkg = "com.maxspeech.android"

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# Drop the old dual-install leftover if present (one-time cleanup).
$legacy = (& $adb shell pm path com.maxspeech.android.debug 2>$null | Out-String).Trim()
if ($legacy) {
    Write-Step "Removing leftover com.maxspeech.android.debug"
    & $adb uninstall com.maxspeech.android.debug | Out-Null
}

if (-not $SkipBuild) {
    Write-Step "Building $variant"
    Push-Location $androidDir
    try {
        & .\gradlew.bat $task --quiet
        if ($LASTEXITCODE -ne 0) { throw "Gradle failed ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
    if (-not (Test-Path $apk)) { throw "APK missing: $apk" }
    Write-Host "APK: $apk ($( [math]::Round((Get-Item $apk).Length / 1MB, 1) ) MB)"
}

Write-Step "Updating $pkg in place (install -r)"
& $adb install -r $apk
if ($LASTEXITCODE -ne 0) { throw "adb install failed" }

$pidNow = (& $adb shell pidof $pkg 2>$null | Out-String).Trim()
if ($Launch -or -not $pidNow) {
    Write-Step "Launching $pkg"
    & $adb shell am start -n "$pkg/com.maxspeech.android.MainActivity" | Out-Null
} else {
    Write-Step "App already running (pid=$pidNow) - left open; no force-stop"
}

Write-Step "Watching logs for ${WatchSeconds}s"
& $adb logcat -c | Out-Null
Start-Sleep -Seconds $WatchSeconds

$dump = & $adb logcat -d -t 400
$dump | Out-File -FilePath (Join-Path $root "android-last-logcat.txt") -Encoding utf8

$crashPatterns = @(
    "FATAL EXCEPTION",
    "AndroidRuntime: FATAL",
    "Uncaught crash",
    "ViewTreeLifecycleOwner not found",
    "FloatingMic: ensureShown: attach failed",
    "promoteForeground failed"
)
$goodPatterns = @(
    "attach: window added",
    "ensureShown: attached ok",
    "ensureShown: already attached",
    "Floating mic: showing",
    "promoteForeground: ok",
    "OverlayService: start requested"
)

$hits = @()
foreach ($p in $crashPatterns) {
    $m = $dump | Select-String -SimpleMatch $p
    if ($m) { $hits += $m }
}

$goods = @()
foreach ($p in $goodPatterns) {
    $m = $dump | Select-String -SimpleMatch $p
    if ($m) { $goods += $m }
}

Write-Host "`n--- healthy signals ---" -ForegroundColor Green
if ($goods.Count -eq 0) {
    Write-Host "(none this window - app may already have been running)" -ForegroundColor Yellow
} else {
    $goods | Select-Object -Last 15 | ForEach-Object { Write-Host $_.Line }
}

Write-Host "`n--- failure signals ---" -ForegroundColor Red
if ($hits.Count -eq 0) {
    Write-Host "(none)" -ForegroundColor Green
} else {
    $hits | Select-Object -Last 25 | ForEach-Object { Write-Host $_.Line }
}

$pidNow = (& $adb shell pidof $pkg 2>$null | Out-String).Trim()
$svc = & $adb shell dumpsys activity services $pkg 2>&1 | Out-String
$fg = ($svc -match "OverlayService") -and ($svc -match "isForeground=true")

Write-Host "`n--- status ---"
if ($pidNow) { Write-Host "pid=$pidNow" } else { Write-Host "pid=DEAD" }
Write-Host "package=$pkg"
Write-Host "overlayServiceFg=$fg"
Write-Host "Full dump: android-last-logcat.txt"

$failed = ($hits.Count -gt 0) -or (-not $pidNow)
if ($failed) {
    Write-Host "`nSHIP FAILED - fix from logs above, then re-run." -ForegroundColor Red
    exit 1
}

Write-Host "`nSHIP OK (in-place update of $pkg)" -ForegroundColor Green
exit 0
