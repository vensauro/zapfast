param (
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# Locate repository root robustly
$repoRoot = (git rev-parse --show-toplevel 2>$null)
if (-not $repoRoot -or -not (Test-Path $repoRoot)) {
    $repoRoot = (Resolve-Path "$PSScriptRoot\..\..\..\..").Path
}
Set-Location $repoRoot

if (-not $SkipBuild) {
    Write-Host "==> Building optimized release binary (cargo build --release)..." -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Cargo build failed with exit code $LASTEXITCODE."
        exit $LASTEXITCODE
    }
}

$releaseBinary = Join-Path $repoRoot "target\release\zapfast.exe"
if (-not (Test-Path $releaseBinary)) {
    Write-Error "Release binary not found at $releaseBinary"
    exit 1
}

$installDir = Join-Path $env:LOCALAPPDATA "Programs\ZapFast"
$targetExe = Join-Path $installDir "zapfast.exe"
$backupExe = Join-Path $installDir "zapfast.exe.old"

if (-not (Test-Path $installDir)) {
    Write-Host "Creating installation directory: $installDir" -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

Write-Host "==> Checking for running ZapFast instances..." -ForegroundColor Cyan
$running = Get-Process -Name zapfast -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Stopping running ZapFast process (PID(s): $($running.Id -join ', '))..." -ForegroundColor Yellow
    Stop-Process -Name zapfast -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

if (Test-Path $targetExe) {
    Write-Host "==> Backing up existing executable to $backupExe..." -ForegroundColor Cyan
    Copy-Item -Path $targetExe -Destination $backupExe -Force
}

Write-Host "==> Installing new binary to $targetExe..." -ForegroundColor Cyan
Copy-Item -Path $releaseBinary -Destination $targetExe -Force

Write-Host "==> Relaunching ZapFast..." -ForegroundColor Green
Start-Process -FilePath $targetExe
Start-Sleep -Seconds 3

$newProcess = Get-Process -Name zapfast -ErrorAction SilentlyContinue
if ($newProcess) {
    Write-Host "ZapFast is running successfully! (PID(s): $($newProcess.Id -join ', '))" -ForegroundColor Green
    $newProcess | Select-Object Id, ProcessName, WorkingSet64, StartTime | Format-Table -AutoSize
} else {
    Write-Warning "ZapFast was launched, but no running process was detected. Check Event Viewer or logs."
}
