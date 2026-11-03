---
name: zapfast-update-windows
description: >-
  Build the optimized release binary of ZapFast, stop any running ZapFast process, back up and replace the installed executable in LocalAppData, and relaunch the application on Windows.
---

# ZapFast Windows Build & Local Update

Use this skill when building an optimized release binary of ZapFast on Windows and updating the local per-user installation (`%LOCALAPPDATA%\Programs\ZapFast`).

## Overview

- **Source binary:** `target\release\zapfast.exe`
- **Installation location:** `$env:LOCALAPPDATA\Programs\ZapFast\zapfast.exe`
- **Backup location:** `$env:LOCALAPPDATA\Programs\ZapFast\zapfast.exe.old`

---

## One-Step Execution (PowerShell Script)

You can run the bundled helper script directly:

```powershell
powershell -ExecutionPolicy Bypass -File .agents/skills/zapfast-update-windows/scripts/update-windows.ps1
```

Or if the binary is already built and you only want to install and relaunch:
```powershell
powershell -ExecutionPolicy Bypass -File .agents/skills/zapfast-update-windows/scripts/update-windows.ps1 -SkipBuild
```

---

## Step-by-Step Manual Procedure

### 1. Build the Release Binary
Run Cargo with the `--release` flag to produce the optimized executable:
```powershell
cargo build --release
```
Verify that `target\release\zapfast.exe` exists and has been built recently.

### 2. Stop Any Running Instance
Check and stop any running `zapfast` process to ensure the executable file lock is released:
```powershell
$running = Get-Process -Name zapfast -ErrorAction SilentlyContinue
if ($running) {
    Stop-Process -Name zapfast -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}
```

### 3. Back Up Current Installation Executable
Back up the existing installation binary to `.old`:
```powershell
$installDir = "$env:LOCALAPPDATA\Programs\ZapFast"
$exePath = "$installDir\zapfast.exe"
$backupPath = "$installDir\zapfast.exe.old"

if (Test-Path $exePath) {
    Copy-Item -Path $exePath -Destination $backupPath -Force
}
```

### 4. Replace with the Newly Built Binary
Copy the release binary to the installation directory:
```powershell
$newExePath = "$PWD\target\release\zapfast.exe"
Copy-Item -Path $newExePath -Destination $exePath -Force
```

### 5. Relaunch and Verify
Start the application and verify the new process is up and running:
```powershell
Start-Process -FilePath $exePath
Start-Sleep -Seconds 3
Get-Process -Name zapfast
```
Confirm the process table displays an active `zapfast` process with a recent start time and non-zero memory footprint.
