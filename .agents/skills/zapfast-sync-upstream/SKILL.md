---
name: zapfast-sync-upstream
description: >-
  Synchronize the ZapFast fork with the upstream repository, rebase custom features (Whisper voice transcription, AI chat summarization, README documentation), validate with targeted tests, push to origin, and update the local Windows release installation.
---

# ZapFast Upstream Synchronization & Windows Update

Use this skill when synchronizing the local fork with upstream (`crmne/zapfast`), rebasing custom fork features on top of `upstream/main`, verifying builds/tests, and updating the local Windows installation.

## Repository & Branch Context

- **Fork Remote (`origin`):** `https://github.com/vensauro/zapfast.git` (branch `main`)
- **Upstream Remote (`upstream`):** `https://github.com/crmne/zapfast.git` (branch `main`)
- **Custom Features to Preserve:**
  1. **Whisper Voice Transcription:** Local offline speech recognition using `whisper-rs` (`src/whisper.rs`, audio bubble UI transcriptions, settings toggles/model picker, auto-transcribe).
  2. **AI Chat Summarization:** Summarizes unread messages/time intervals via OpenAI-compatible endpoints (`src/openai.rs`, summarization modal dialog, chat header and context menu entrypoints, settings configuration).
  3. **README Documentation:** Sections documenting the fork's features.

---

## Step 1: Safety Backup

1. Check working directory status:
   ```powershell
   git status
   ```
   Ensure working tree is clean before proceeding.

2. Fetch all remotes with pruning:
   ```powershell
   git fetch --all --prune
   ```

3. Create a timestamped backup branch and push to `origin`:
   ```powershell
   $today = (Get-Date).ToString("yyyyMMdd")
   git branch "backup-main-$today"
   git push origin "backup-main-$today"
   ```

---

## Step 2: Rebase onto Upstream

1. Start rebase onto `upstream/main`:
   ```powershell
   git rebase upstream/main
   ```

2. When resolving conflicts:
   - **`Cargo.lock`:** Checkout either version (`git checkout --ours Cargo.lock`) and run `cargo check` so Cargo regenerates the lockfile cleanly with all dependencies.
   - **`src/ui/conversation.rs`:**
     - Preserve upstream message height/layout additions (e.g. `DisappearingTimer`) while keeping `transcription` height on `Content::Audio`.
     - In context menu actions, preserve upstream's keyboard menu items (`inner_entries`, `copyable_text`), ensuring `Content::Audio` transcription is included in `copyable_text`.
   - **`src/app.rs`:**
     - Preserve dialog handling for upstream additions (e.g. `Dialog::BlockedContacts`, `Dialog::ConfirmBlock`) AND custom dialogs (`Dialog::Summarize`).
   - **`src/archive.rs`:**
     - Preserve new upstream columns on messages (such as `history_order`) in `unread_messages()` and `messages_since()` queries.
   - **Custom modules:** Never drop `src/whisper.rs` or `src/openai.rs`.

3. Windows Git Editor Note:
   If git prompts or fails with `editor 'false'`, supply a non-interactive editor when continuing:
   ```powershell
   powershell -NoProfile -Command "& { [Environment]::SetEnvironmentVariable('GIT_EDITOR', ':', 'Process'); git rebase --continue }"
   ```

---

## Step 3: Validation & Testing

1. Validate compilation:
   ```powershell
   cargo check
   ```

2. Verify all test suites compile:
   ```powershell
   cargo test --no-run
   ```

3. Run targeted custom feature tests:
   ```powershell
   cargo test whisper
   cargo test openai
   cargo test unread_and_since_messages_queries
   ```

---

## Step 4: Sync Remote Fork

Push the rebased commits to `origin main` using `--force-with-lease`:
```powershell
git push origin main --force-with-lease
```

---

## Step 5: Build & Update Windows Installation

1. Build the optimized release binary:
   ```powershell
   cargo build --release
   ```

2. Update installed binary and restart ZapFast:
   ```powershell
   Stop-Process -Name zapfast -Force -ErrorAction SilentlyContinue
   Start-Sleep -Seconds 2

   $installDir = "$env:LOCALAPPDATA\Programs\ZapFast"
   $exePath = "$installDir\zapfast.exe"
   $backupPath = "$installDir\zapfast.exe.old"
   $newExePath = "$PWD\target\release\zapfast.exe"

   Copy-Item -Path $exePath -Destination $backupPath -Force
   Copy-Item -Path $newExePath -Destination $exePath -Force

   Start-Process -FilePath $exePath
   Start-Sleep -Seconds 3

   Get-Process -Name zapfast
   ```
