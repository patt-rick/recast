# Recast Plan 1 — Conversion Core + Companion Window

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri 2 tray app for Windows where you drop files on the companion window's Activity view, pick a conversion from a list, and get a correctly named output next to the original. Engines download on demand; every job has real progress, can be cancelled and never hangs.

**Architecture:** A Rust library crate (`recast_lib`) holds all logic as plain modules with no Tauri dependency (settings, detect, presets, engines, jobs, history), each testable with `cargo test`. A thin Tauri layer (`app_state.rs`, `ipc.rs`, `tray.rs`) wires them to a React + TypeScript companion window. Engines are external CLI programs run with argv arrays inside a Windows Job Object, in a per-job hidden temp directory that is renamed into place on success.

**Tech Stack:** Tauri 2, Rust (stable, MSVC), React 19 + TypeScript + Vite, Vitest, rusqlite (bundled SQLite), reqwest (blocking), `windows` crate, 7-Zip (`7z.exe` + `7z.dll`, bundled), insta (snapshot tests).

**Spec:** `docs/superpowers/specs/2026-11-18-recast-design.md`. Section references (§) point there.

---

## Scope of this plan

Plan 1 delivers spec §5 (whole catalog), §6.1–6.6, §6.11 (except `capture_backdrop`), §7 (all rows except backdrop and shake), §8, and the Activity, Library and Settings parts of §4.5.

**Deferred to Plan 2 (pad):** the ring UI (§4.2–4.4), `geometry.ts`/`hitTest.ts`, backdrop capture (§6.8), thumbnails, the GIF ">60 s" warning, the Tools page (§4.5 item 2, reorderable Merge / Images → PDF, Split page ranges).
**Deferred to Plan 3 (triggers & shell):** edge handle, desk pad, shake (§4.1, §6.7), Explorer entry + single instance (§6.10), first-run welcome (§4.6), start with Windows, notification action buttons (Show in folder / Details / Save to fallback folder), tray glyphs, NSIS uninstall hook (§6.12), `docs/qa-checklist.md`, and the settings UI for triggers/Explorer/start-with-Windows.

In Plan 1, the Activity view shows a plain list of the available presets instead of the ring.

## Deliberate deviations from the spec (fold back into the spec when Plan 1 lands)

1. **7-Zip, not `7za`.** `7za.exe` cannot unpack NSIS installers; only the full `7z.exe` + `7z.dll` can. Recast ships those two files (LGPL-2.1 + BSD-3 + unRAR restriction) and uses them for **every** engine archive (zip, 7z, NSIS), so no Rust zip crate is needed.
2. **Engines run with the job's temp dir as their working directory.** `{out}` and `{out_dir}` expand to the *relative* names `out.<ext>` and `out`. ImageMagick and Ghostscript inputs are **staged** (hard-linked, or copied across volumes) into the temp dir as `in-1.<ext>`, … This makes `%`, `[ ]`, leading `-`, Unicode and >260-char paths safe for the two engines that interpret filenames.
3. **`out_naming` gains `fixed:<name>`** (`merged.pdf`, `combined.pdf`).
4. **Preset fields** beyond §5: `bin`, `passes` (list of argv templates; Shrink has two), `out_format` (canonical override, e.g. `alac`), `keep_same_format` (the data-driven form of the Shrink/Compress exception), `hide_when_multi`, `discard_if_not_smaller`, `stage_inputs`, `extra_outputs`, `ok_exit` (qpdf exits 3 on warnings).
5. **Manifest `bins`** map a logical name to an exe *file name*; the installer locates it inside the extracted tree and records the relative path and SHA-256. Entries may also carry `check` (a post-install smoke command) and `capabilities` (`magick_formats`).
6. **No generic `reveal` / `open_path` commands.** The webview could pass them any path, so they were replaced by `reveal_job(id)` / `open_job(id)` (the backend looks up the history row and requires status `done` and an existing output) and `reveal_notice(index)` (licence files). TS: `ipc.revealJob`, `ipc.openJob`, `ipc.revealNotice`.
7. The Windows platform implementation lives in `platform/win.rs` (a module named `windows` would shadow the `windows` crate).
8. **Download resume cache.** A failed engine install leaves no engine folder, but a verified or partly downloaded archive stays in `engines\.partial\<id>\` so the next attempt resumes instead of re-downloading. A checksum mismatch deletes it.
9. **Stall watchdog arming.** For ffmpeg, the stall timer only runs between the first real output timestamp and 99% of the duration. Before and after that (GIF palettegen, `+faststart`), only the per-file hard cap applies.
10. **Engine waits are cancellable; the install isn't.** Cancelling a job that is waiting for an engine download ends the job at once; the download continues for later jobs. HTTP reads time out after 60 s idle.
11. **Single instance arrives early.** `tauri-plugin-single-instance` is registered in Plan 1 (it only focuses the window), because a second copy's crash recovery would delete the first copy's live temp dirs. Plan 3 adds argv forwarding.
12. **Batch progress** ("n/total files", §6.4) is shown per job in Activity for now; the pad's batch ring arrives in Plan 2.
13. **Preset field `skip_formats`** (canonical input formats a preset is hidden for): Shrink is hidden for GIF and ICO.
14. **ICO pads to square** (resize to 256x256, centred, transparent) instead of stretching; image Shrink discards results that aren't smaller (`discard_if_not_smaller`).
15. **settings.json read errors** other than "missing" never lead to overwriting the file (the loaded settings are read-only); a corrupt file is kept as `settings.json.bad`.
16. **MOV re-encodes** to H.264/AAC rather than remuxing, and pandoc may fetch remote images that a document links to (§8 amended).
17. **ImageMagick hardening is part of the install.** `policy.xml` (now with resource limits: memory 1 GiB, map 2 GiB, disk 4 GiB, width/height 32 KP, area 512 MP) and the capabilities file are written into the staging folder before the atomic rename, via an install post-unpack hook; if either fails nothing is installed, and a missing one later makes the engine Damaged. Every magick run (jobs and the capability probe) sets `MAGICK_CONFIGURE_PATH` to magick.exe's folder (`RunSpec.env`).
18. **Engine leases instead of an in-use check.** A job takes a shared lease on its engine after `ensure` and holds it until the job ends. Library Download/Update/Repair/Remove need exclusive access and are refused ("A conversion is using this converter…") while any lease is held; a job's `ensure` that must repair a Damaged engine waits for running conversions instead. `JobManager::engine_in_use` is gone.
19. **Engines start suspended.** Engine and helper processes (jobs, captures, 7-Zip unpacking) are created with `CREATE_SUSPENDED`, put in their Job Object, then resumed, so no grandchild can start outside the kill group. Pipe-reader threads are joined for at most 2 s, then detached. 7-Zip unpacking has a 10-minute cap.
20. **7-Zip 26.06, pinned by SHA-256.** `scripts/fetch-7zip.mjs` pins release 26.06 (`7zr.exe` and `7z2606-x64.exe` from github.com/ip7z/7zip) by SHA-256; 23.01 had known CVEs. Move to a newer release by changing `VERSION`, `RELEASE` and both hashes together, and check that `7z i` still lists Nsis.

## File structure

```
recast/
  package.json, tsconfig.json, vite.config.ts, index.html, LICENSE, README.md
  scripts/
    fetch-7zip.mjs           # downloads 7-Zip, extracts 7z.exe/7z.dll/License.txt into src-tauri/bin
    refresh-engines.mjs      # pins engine URLs + sha256 into engines.toml, runs capability checks
  src/
    main.tsx                 # mounts the companion app
    shared/types.ts          # TS mirror of the Rust IPC types
    shared/ipc.ts            # invoke wrappers + event listeners
    shared/format.ts         # formatBytes, formatEta (+ format.test.ts)
    companion/App.tsx        # sidebar shell
    companion/jobs.ts        # job list reducer (+ jobs.test.ts)
    companion/Activity.tsx   # drop target, preset list, running jobs, history
    companion/Library.tsx    # engines + third-party notices
    companion/Settings.tsx   # quality/output/jobs settings
    companion/companion.css
  src-tauri/
    Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
    bin/7z.exe, bin/7z.dll, bin/7zip-License.txt
    presets/presets.toml     # the catalog (§5)
    engines/engines.toml     # the manifest (§6.3), written by scripts/refresh-engines.mjs
    examples/fake_engine.rs  # test double for engine processes
    tests/engines_integration.rs   # real engines, behind `--features engines`
    src/
      main.rs, lib.rs
      paths.rs               # %LOCALAPPDATA%\Recast, %APPDATA%\Recast
      settings.rs            # typed settings, defaults, tolerant load, patch, vars
      detect.rs              # kinds, canonical formats, ffprobe JSON
      fsutil.rs              # sha256_file, walk_files, dir_size
      presets/mod.rs         # Catalog, Preset, OutNaming, validation
      presets/naming.rs      # output names and collisions
      presets/command.rs     # placeholder expansion → argv
      presets/available.rs   # which presets apply to a set of files
      engines/mod.rs         # Engines manager (status, ensure, bins, probe, caps)
      engines/manifest.rs    # engines.toml model + validation
      engines/install.rs     # fetch, verify, extract, locate, record
      engines/caps.rs        # `magick -list format` parser
      jobs/mod.rs            # JobManager: queue, execute, finalize
      jobs/progress.rs       # ffmpeg / ghostscript progress trackers
      jobs/runner.rs         # spawn in Job Object, drain pipes, watchdog
      jobs/explain.rs        # stderr → plain-language reason
      jobs/math.rs           # shrink bitrate, disk estimates
      jobs/notify.rs         # done/failed/batch notifications
      history.rs             # SQLite job history
      pad_model.rs           # inspect(paths) → PadModel
      app_state.rs           # Tauri setup: build state, logging, recovery
      ipc.rs                 # #[tauri::command]s + Tauri event sink
      tray.rs                # tray icon + menu
      platform/mod.rs, platform/win.rs   # Job Objects, disk space, hidden attr, no-replace move
      testutil.rs            # (cfg(test)) fake engine path, temp helpers
```

**Conventions for every task**
- Rust commands run from `src-tauri/`; npm commands from the repo root. Use the Bash tool (Git Bash).
- **Always run plain `cargo test`** (optionally with a name filter after `--`, e.g. `cargo test settings::`). Do **not** use `--lib`: plain `cargo test` also builds `examples/fake_engine.rs`, which the runner and job tests spawn.
- Commit after each task with the message given. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

### Task 1: Scaffold the app

**Files:**
- Create: `package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`, `src/main.tsx`, `LICENSE`, `README.md`
- Create (by `tauri init`, then edit): `src-tauri/**`
- Modify: `.gitignore`

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "recast",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "license": "MIT",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run",
    "tauri": "tauri"
  }
}
```

- [ ] **Step 2: Install front-end dependencies**

```bash
npm i react react-dom @tauri-apps/api @tauri-apps/plugin-notification @tauri-apps/plugin-opener
npm i -D vite @vitejs/plugin-react typescript @types/react @types/react-dom vitest jsdom @tauri-apps/cli
```
Expected: both finish with no `ERR!` lines.

- [ ] **Step 3: Write `tsconfig.json`, `vite.config.ts`, `index.html`, `src/main.tsx`**

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noEmit": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "types": ["vitest/globals"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`vite.config.ts`:
```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "chrome105", outDir: "dist" },
  test: { environment: "jsdom", globals: true },
});
```

`index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Recast</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx` (temporary; replaced in Task 18):
```tsx
import { createRoot } from "react-dom/client";

createRoot(document.getElementById("root")!).render(<p>Recast</p>);
```

- [ ] **Step 4: Initialise Tauri**

```bash
npx tauri init --ci --app-name recast --window-title Recast --frontend-dist ../dist --dev-url http://localhost:1420 --before-dev-command "npm run dev" --before-build-command "npm run build"
```
Expected: `src-tauri/` exists with `Cargo.toml`, `tauri.conf.json`, `src/main.rs`, `src/lib.rs`, `build.rs`, `capabilities/default.json`, `icons/`.
Open `src-tauri/Cargo.toml` and confirm `[lib] name = "recast_lib"`. If the generated name differs, set it to `recast_lib` and make `src/main.rs` call `recast_lib::run()`.

- [ ] **Step 5: Replace `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Recast",
  "version": "0.1.0",
  "identifier": "dev.recast.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "label": "companion",
        "title": "Recast",
        "width": 760,
        "height": 520,
        "minWidth": 640,
        "minHeight": 420,
        "visible": true,
        "dragDropEnabled": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"],
    "windows": {
      "webviewInstallMode": { "type": "downloadBootstrapper" },
      "nsis": { "installMode": "currentUser" }
    }
  }
}
```

Replace `src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Companion window permissions",
  "windows": ["companion"],
  "permissions": ["core:default", "notification:default", "opener:default"]
}
```

- [ ] **Step 6: Generate the icon set from the locked master (§12)**

```bash
npx tauri icon assets/icon/recast-icon.svg
```
Expected: `src-tauri/icons/icon.ico`, `32x32.png`, `128x128.png`, `128x128@2x.png` regenerated. If the CLI rejects SVG input, render a 1024 px PNG first (`npx --yes @resvg/resvg-js-cli assets/icon/recast-icon.svg .cache/icon.png`) and pass that PNG instead.

- [ ] **Step 7: Write `LICENSE` (MIT, D12) and `README.md`**

`LICENSE`:
```
MIT License

Copyright (c) 2026 Patrick Ackom

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

`README.md`:
```markdown
# Recast

Drop a file, pick a conversion, get the result next to the original. Windows 10/11.
Recast wraps FFmpeg, ImageMagick, Pandoc, qpdf and Ghostscript, which download the first time they're needed.

Design: `docs/superpowers/specs/2026-11-18-recast-design.md`

## Develop

    npm install
    npm run tauri dev      # run the app
    npm test               # front-end tests
    cd src-tauri && cargo test                     # Rust tests (no engines needed)
    cd src-tauri && cargo test --features engines  # real-engine tests (downloads ~250 MB once)

Recast's own code is MIT. Engines keep their own licences (Library → Third-party notices).
```

- [ ] **Step 8: Extend `.gitignore`**

Append:
```
.cache/
*.snap.new
src-tauri/gen/schemas/
```

- [ ] **Step 9: Verify both halves build**

```bash
npm run build
cd src-tauri && cargo build
```
Expected: `dist/index.html` exists; cargo ends with `Finished`. (First build takes several minutes.)

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "chore: scaffold Tauri 2 + React app with MIT licence and icon set"
```

---

### Task 2: Dependencies, module skeleton and the `platform` module

**Files:**
- Modify: `src-tauri/Cargo.toml` (via `cargo add`), `src-tauri/src/lib.rs`
- Create: `src-tauri/src/platform/mod.rs`, `src-tauri/src/platform/win.rs`

- [ ] **Step 1: Add crates** (from `src-tauri/`)

```bash
cargo add tauri -F tray-icon
cargo add serde -F derive
cargo add serde_json toml infer dirs log flexi_logger sha2 hex
cargo add uuid -F v4
cargo add rusqlite -F bundled
cargo add reqwest -F blocking
cargo add tauri-plugin-notification tauri-plugin-opener tauri-plugin-single-instance
cargo add windows -F Win32_Foundation,Win32_Security,Win32_System_JobObjects,Win32_System_Threading,Win32_Storage_FileSystem
cargo add --dev tempfile insta
```

Then add to `Cargo.toml`:
```toml
[features]
# Runs tests/engines_integration.rs against real, downloaded engines.
engines = []
```

- [ ] **Step 2: Write the failing platform tests** — `src-tauri/src/platform/win.rs` (tests first; implementation in Step 4)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn kill_group_terminates_assigned_process() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let group = KillGroup::new().unwrap();
        group.assign(&child).unwrap();
        let pid = child.id();
        assert!(is_process_alive(pid));
        group.kill();
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert!(!is_process_alive(pid));
    }

    #[test]
    fn free_space_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(dir.path()).unwrap() > 0);
    }

    #[test]
    fn set_hidden_marks_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join(".recast-x");
        std::fs::create_dir(&sub).unwrap();
        set_hidden(&sub).unwrap();
        assert!(is_hidden(&sub));
    }

    #[test]
    fn move_no_replace_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let err = move_no_replace(&a, &b).unwrap_err();
        assert!(is_already_exists(&err));
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        let c = dir.path().join("c.txt");
        move_no_replace(&a, &c).unwrap();
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "a");
    }

    #[test]
    fn long_paths_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().to_path_buf();
        while deep.as_os_str().len() < 300 {
            deep.push("a-fairly-long-folder-name-for-testing");
        }
        std::fs::create_dir_all(&deep).unwrap(); // std handles long paths itself
        let from = deep.join("in.txt");
        std::fs::write(&from, "x").unwrap();
        let to = deep.join("out.txt");
        move_no_replace(&from, &to).unwrap();
        assert!(to.exists());
        set_hidden(&deep).unwrap();
        assert!(is_hidden(&deep));
        assert!(free_space(&deep).is_some());
    }
}
```

- [ ] **Step 3: Write `platform/mod.rs` and the `lib.rs` skeleton**

`src-tauri/src/platform/mod.rs`:
```rust
//! The only module allowed to call OS APIs (spec §6.9).
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use self::win::*;

#[cfg(not(windows))]
compile_error!("Recast v1 targets Windows only; add a platform implementation (spec §6.9).");
```

`src-tauri/src/lib.rs` (keep the generated `run()` body for now; Task 16 replaces it):
```rust
pub mod platform;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
```

Run: `cargo test platform::`
Expected: FAIL to compile (`KillGroup`, `free_space`, … not found).

- [ ] **Step 4: Implement `platform/win.rs`** (above the tests module)

```rust
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Child;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetFileAttributesW, MoveFileExW, SetFileAttributesW,
    FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES, INVALID_FILE_ATTRIBUTES, MOVE_FILE_FLAGS,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// `CREATE_NO_WINDOW`: engine consoles never flash on screen.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// NUL-terminated UTF-16 path. Absolute paths of 248+ chars get the `\\?\`
/// (or `\\?\UNC\`) prefix so these Win32 calls work past MAX_PATH.
fn wide(p: &Path) -> Vec<u16> {
    const BS: u16 = b'\\' as u16;
    let raw: Vec<u16> = p.as_os_str().encode_wide().map(|c| if c == b'/' as u16 { BS } else { c }).collect();
    let verbatim = raw.starts_with(&[BS, BS, b'?' as u16, BS]);
    let mut out: Vec<u16> = if raw.len() >= 248 && !verbatim && p.is_absolute() {
        if raw.starts_with(&[BS, BS]) {
            r"\\?\UNC".encode_utf16().chain(raw[1..].iter().copied()).collect()
        } else {
            r"\\?\".encode_utf16().chain(raw.iter().copied()).collect()
        }
    } else {
        raw
    };
    out.push(0);
    out
}

fn to_io(e: windows::core::Error) -> io::Error {
    io::Error::other(e.to_string())
}

/// A Windows Job Object with KILL_ON_JOB_CLOSE: killing it (or dropping it,
/// or Recast crashing) kills every process assigned to it and their children.
pub struct KillGroup {
    raw: usize,
}

impl KillGroup {
    pub fn new() -> io::Result<Self> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).map_err(to_io)?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if let Err(e) = set {
                let _ = CloseHandle(job);
                return Err(to_io(e));
            }
            Ok(Self { raw: job.0 as usize })
        }
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.raw as *mut c_void)
    }

    /// Assign right after spawn. Engines don't spawn children in their first
    /// instructions, so the window before assignment is harmless in practice.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        unsafe { AssignProcessToJobObject(self.handle(), HANDLE(child.as_raw_handle())).map_err(to_io) }
    }

    pub fn kill(&self) {
        unsafe {
            let _ = TerminateJobObject(self.handle(), 1);
        }
    }
}

impl Drop for KillGroup {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle());
        }
    }
}

/// Bytes available to the current user on the volume holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    let w = wide(path);
    let mut avail = 0u64;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail as *mut u64), None, None).ok()? };
    Some(avail)
}

pub fn set_hidden(path: &Path) -> io::Result<()> {
    let w = wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(w.as_ptr()));
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        SetFileAttributesW(PCWSTR(w.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(attrs | FILE_ATTRIBUTE_HIDDEN.0))
            .map_err(to_io)
    }
}

pub fn is_hidden(path: &Path) -> bool {
    let w = wide(path);
    let attrs = unsafe { GetFileAttributesW(PCWSTR(w.as_ptr())) };
    attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_HIDDEN.0 != 0
}

/// Rename a file or folder on the same volume, failing if `to` exists
/// (std::fs::rename would silently replace an existing file on Windows).
pub fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let (f, t) = (wide(from), wide(to));
    unsafe { MoveFileExW(PCWSTR(f.as_ptr()), PCWSTR(t.as_ptr()), MOVE_FILE_FLAGS(0)) }
        .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
}

/// ERROR_FILE_EXISTS (80) or ERROR_ALREADY_EXISTS (183).
pub fn is_already_exists(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(80) | Some(183))
}

/// Access denied (5), sharing violation (32), lock violation (33): what
/// antivirus, OneDrive and Dropbox cause while they briefly hold a new file.
pub fn is_transient_lock(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33))
}

/// Diagnostics for tests: is a process with this pid still running?
pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false.into(), pid) {
            Ok(h) => {
                let mut code = 0u32;
                let ok = GetExitCodeProcess(h, &mut code).is_ok();
                let _ = CloseHandle(h);
                ok && code == STILL_ACTIVE.0 as u32
            }
            Err(_) => false,
        }
    }
}
```

> If the installed `windows` crate version differs in a signature (e.g. a `BOOL` vs `bool` parameter, or `Option<*mut u64>`), follow the compiler's suggestion; the semantics above are what matter.

- [ ] **Step 5: Run the tests**

Run: `cargo test platform::`
Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(platform): job-object kill groups, disk space, hidden attr, no-replace move"
```

---

### Task 3: `paths` and `settings`

**Files:**
- Create: `src-tauri/src/paths.rs`, `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod paths; pub mod settings;`)

- [ ] **Step 1: Write `paths.rs`** (no logic worth a test)

```rust
use std::path::{Path, PathBuf};

/// Recast's fixed data folders (spec §6.12): not Tauri's identifier-named ones.
#[derive(Clone, Debug)]
pub struct Paths {
    /// %LOCALAPPDATA%\Recast — engines, history, logs.
    pub local: PathBuf,
    /// %APPDATA%\Recast — settings.
    pub roaming: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        Self {
            local: dirs::data_local_dir().expect("LOCALAPPDATA is not set").join("Recast"),
            roaming: dirs::config_dir().expect("APPDATA is not set").join("Recast"),
        }
    }

    pub fn under(root: &Path) -> Self {
        Self { local: root.join("local"), roaming: root.join("roaming") }
    }

    pub fn engines(&self) -> PathBuf {
        self.local.join("engines")
    }
    pub fn db(&self) -> PathBuf {
        self.local.join("recast.db")
    }
    pub fn logs(&self) -> PathBuf {
        self.local.join("logs")
    }
    pub fn settings(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }
}
```

- [ ] **Step 2: Write the failing settings tests** — bottom of `settings.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_spec_section_10() {
        let s = Settings::default();
        assert!(s.triggers.edge_handle && !s.triggers.desk_pad && s.triggers.shake);
        assert_eq!(s.triggers.edge, "left");
        assert_eq!(s.triggers.edge_pos, 0.5);
        assert!(s.shell.explorer_entry);
        assert_eq!(s.output.location, "next_to_original");
        assert_eq!((s.quality.mp3_kbps, s.quality.aac_kbps), (192, 192));
        assert_eq!((s.quality.jpg_q, s.quality.webp_q, s.quality.avif_q), (85, 82, 60));
        assert_eq!((s.gif.fps, s.gif.width), (12, 480));
        assert_eq!((s.shrink.target_mb, s.shrink.image_px), (25, 2048));
        assert_eq!(s.jobs.concurrency, 2);
        assert_eq!((s.jobs.stall_timeout_s, s.jobs.no_progress_cap_min), (60, 10));
        assert!(s.app.start_with_windows && !s.app.first_run_done);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let l = load(&dir.path().join("settings.json"));
        assert_eq!(l.settings, Settings::default());
        assert!(l.warnings.is_empty());
    }

    #[test]
    fn wrong_types_and_out_of_range_values_fall_back_with_warnings() {
        let l = from_value(json!({
            "jobs": { "concurrency": 9, "stall_timeout_s": "soon" },
            "quality": { "mp3_kbps": 320 },
            "triggers": { "edge": "diagonal" }
        }));
        assert_eq!(l.settings.jobs.concurrency, 2);
        assert_eq!(l.settings.jobs.stall_timeout_s, 60);
        assert_eq!(l.settings.quality.mp3_kbps, 320);
        assert_eq!(l.settings.triggers.edge, "left");
        assert_eq!(l.warnings.len(), 3, "{:?}", l.warnings);
    }

    #[test]
    fn unknown_fields_survive_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"future_flag":true,"quality":{"mp3_kbps":256,"later":1}}"#).unwrap();
        let l = load(&path);
        save(&path, &l).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["future_flag"], json!(true));
        assert_eq!(v["quality"]["later"], json!(1));
        assert_eq!(v["quality"]["mp3_kbps"], json!(256));
        assert_eq!(v["jobs"]["concurrency"], json!(2));
    }

    #[test]
    fn corrupt_file_gives_defaults_and_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        let l = load(&path);
        assert_eq!(l.settings, Settings::default());
        assert_eq!(l.warnings.len(), 1);
    }

    #[test]
    fn patch_changes_one_field_and_is_sanitized() {
        let l = from_value(json!({}));
        let l = apply_patch(&l, json!({ "jobs": { "concurrency": 3 } }));
        assert_eq!(l.settings.jobs.concurrency, 3);
        let l = apply_patch(&l, json!({ "jobs": { "concurrency": 0 } }));
        assert_eq!(l.settings.jobs.concurrency, 2);
    }

    #[test]
    fn vars_are_flattened_with_setting_prefix() {
        let v = Settings::default().to_vars();
        assert_eq!(v["setting.quality.mp3_kbps"], "192");
        assert_eq!(v["setting.gif.width"], "480");
        assert!(var_keys().contains("setting.shrink.image_px"));
    }
}
```

Run: `cargo test settings::`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `settings.rs`** (above the tests)

```rust
//! Typed settings at %APPDATA%\Recast\settings.json (spec §6.6, defaults §10).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    pub version: u32,
    pub triggers: Triggers,
    pub shell: Shell,
    pub output: Output,
    pub quality: Quality,
    pub gif: Gif,
    pub shrink: Shrink,
    pub jobs: Jobs,
    pub app: App,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Triggers { pub edge_handle: bool, pub edge: String, pub edge_pos: f64, pub desk_pad: bool, pub shake: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shell { pub explorer_entry: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Output {
    /// "next_to_original" | "folder"
    pub location: String,
    pub folder: String,
    /// Empty means Documents\Recast.
    pub fallback_folder: String,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quality { pub mp3_kbps: u32, pub aac_kbps: u32, pub jpg_q: u32, pub webp_q: u32, pub avif_q: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Gif { pub fps: u32, pub width: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shrink { pub target_mb: u32, pub image_px: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Jobs { pub concurrency: u32, pub stall_timeout_s: u32, pub no_progress_cap_min: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct App { pub start_with_windows: bool, pub first_run_done: bool }

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            triggers: Triggers { edge_handle: true, edge: "left".into(), edge_pos: 0.5, desk_pad: false, shake: true },
            shell: Shell { explorer_entry: true },
            output: Output { location: "next_to_original".into(), folder: String::new(), fallback_folder: String::new() },
            quality: Quality { mp3_kbps: 192, aac_kbps: 192, jpg_q: 85, webp_q: 82, avif_q: 60 },
            gif: Gif { fps: 12, width: 480 },
            shrink: Shrink { target_mb: 25, image_px: 2048 },
            jobs: Jobs { concurrency: 2, stall_timeout_s: 60, no_progress_cap_min: 10 },
            app: App { start_with_windows: true, first_run_done: false },
        }
    }
}

impl Settings {
    /// Every leaf as `setting.<path>` → string, for preset placeholders.
    pub fn to_vars(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        flatten("setting", &serde_json::to_value(self).expect("settings serialize"), &mut m);
        m
    }

    pub fn fallback_dir(&self) -> PathBuf {
        if self.output.fallback_folder.trim().is_empty() {
            dirs::document_dir().unwrap_or_else(std::env::temp_dir).join("Recast")
        } else {
            PathBuf::from(&self.output.fallback_folder)
        }
    }

    fn sanitize(&mut self, w: &mut Vec<String>) {
        let d = Settings::default();
        macro_rules! range {
            ($f:expr, $def:expr, $lo:expr, $hi:expr, $name:literal) => {
                if !($lo..=$hi).contains(&$f) {
                    w.push(format!("{}: out of range, using default", $name));
                    $f = $def;
                }
            };
        }
        range!(self.quality.mp3_kbps, d.quality.mp3_kbps, 32, 320, "quality.mp3_kbps");
        range!(self.quality.aac_kbps, d.quality.aac_kbps, 32, 320, "quality.aac_kbps");
        range!(self.quality.jpg_q, d.quality.jpg_q, 1, 100, "quality.jpg_q");
        range!(self.quality.webp_q, d.quality.webp_q, 1, 100, "quality.webp_q");
        range!(self.quality.avif_q, d.quality.avif_q, 1, 100, "quality.avif_q");
        range!(self.gif.fps, d.gif.fps, 1, 50, "gif.fps");
        range!(self.gif.width, d.gif.width, 16, 4096, "gif.width");
        range!(self.shrink.target_mb, d.shrink.target_mb, 1, 100_000, "shrink.target_mb");
        range!(self.shrink.image_px, d.shrink.image_px, 16, 16_384, "shrink.image_px");
        range!(self.jobs.concurrency, d.jobs.concurrency, 1, 4, "jobs.concurrency");
        range!(self.jobs.stall_timeout_s, d.jobs.stall_timeout_s, 10, 3600, "jobs.stall_timeout_s");
        range!(self.jobs.no_progress_cap_min, d.jobs.no_progress_cap_min, 1, 240, "jobs.no_progress_cap_min");
        range!(self.triggers.edge_pos, d.triggers.edge_pos, 0.0, 1.0, "triggers.edge_pos");
        if !["left", "right", "top", "bottom"].contains(&self.triggers.edge.as_str()) {
            w.push("triggers.edge: unknown value, using default".into());
            self.triggers.edge = d.triggers.edge;
        }
        if !["next_to_original", "folder"].contains(&self.output.location.as_str()) {
            w.push("output.location: unknown value, using default".into());
            self.output.location = d.output.location;
        }
    }
}

/// Settings plus the raw JSON they came from, so unknown fields survive a save.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub raw: Value,
    pub warnings: Vec<String>,
}

pub fn var_keys() -> BTreeSet<String> {
    Settings::default().to_vars().into_keys().collect()
}

pub fn load(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Err(_) => from_value(Value::Object(Map::new())),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) => from_value(v),
            Err(e) => {
                let mut l = from_value(Value::Object(Map::new()));
                l.warnings.push(format!("settings.json is not valid JSON ({e}); using defaults"));
                l
            }
        },
    }
}

pub fn from_value(raw: Value) -> Loaded {
    let mut warnings = Vec::new();
    let defaults = serde_json::to_value(Settings::default()).expect("defaults serialize");
    let merged = overlay(&defaults, &raw, "", &mut warnings);
    let mut settings: Settings = serde_json::from_value(merged).expect("overlay keeps default types");
    // v1 has no migrations; later versions transform `raw` here before overlaying.
    settings.version = CURRENT_VERSION;
    settings.sanitize(&mut warnings);
    Loaded { settings, raw, warnings }
}

pub fn apply_patch(current: &Loaded, patch: Value) -> Loaded {
    let mut raw = current.raw.clone();
    deep_merge(&mut raw, &patch);
    from_value(raw)
}

pub fn save(path: &Path, l: &Loaded) -> std::io::Result<()> {
    let mut out = l.raw.clone();
    deep_merge(&mut out, &serde_json::to_value(&l.settings).expect("settings serialize"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&out)?)?;
    std::fs::rename(&tmp, path)
}

/// Take `user`'s value wherever its JSON type matches the default's.
fn overlay(default: &Value, user: &Value, path: &str, w: &mut Vec<String>) -> Value {
    match (default, user) {
        (Value::Object(d), Value::Object(u)) => {
            let mut out = Map::new();
            for (k, dv) in d {
                let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                let v = match u.get(k) {
                    Some(uv) => overlay(dv, uv, &p, w),
                    None => dv.clone(),
                };
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        (Value::Bool(_), Value::Bool(_)) | (Value::String(_), Value::String(_)) => user.clone(),
        (Value::Number(dn), Value::Number(un)) => {
            let ok = if dn.is_f64() { true } else { un.as_u64().is_some_and(|v| v <= u32::MAX as u64) };
            if ok {
                user.clone()
            } else {
                w.push(format!("{path}: not a whole number, using default"));
                default.clone()
            }
        }
        _ => {
            w.push(format!("{path}: wrong type, using default"));
            default.clone()
        }
    }
}

fn deep_merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                deep_merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn flatten(prefix: &str, v: &Value, m: &mut BTreeMap<String, String>) {
    match v {
        Value::Object(o) => {
            for (k, v) in o {
                flatten(&format!("{prefix}.{k}"), v, m);
            }
        }
        Value::String(s) => {
            m.insert(prefix.to_string(), s.clone());
        }
        other => {
            m.insert(prefix.to_string(), other.to_string());
        }
    }
}
```

Note the `version` field is a top-level number with default `1`, so it also flows through `overlay`.

- [ ] **Step 4: Run the tests**

Run: `cargo test settings::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(settings): typed settings with tolerant load, patching and preset vars"
```

---

### Task 4: `detect` and `fsutil`

**Files:**
- Create: `src-tauri/src/detect.rs`, `src-tauri/src/fsutil.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod detect; pub mod fsutil;`)

- [ ] **Step 1: Write `fsutil.rs`** (with its tests)

```rust
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Every file under `root`, recursively (no symlink following).
pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                out.push(e.path());
            }
        }
    }
    out
}

pub fn dir_size(root: &Path) -> u64 {
    walk_files(root).iter().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum()
}

/// fs::rename, retried with backoff (100 ms, doubling) while antivirus or sync
/// clients hold a transient lock, up to `budget` in total.
pub fn rename_with_retry(from: &Path, to: &Path, budget: std::time::Duration) -> io::Result<()> {
    let budget = budget.as_millis() as u64;
    let mut wait = 100u64;
    let mut waited = 0u64;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if crate::platform::is_transient_lock(&e) && waited < budget => {
                let d = wait.min(budget - waited);
                std::thread::sleep(std::time::Duration::from_millis(d));
                waited += d;
                wait *= 2;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_walks() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("sub")).unwrap();
        fs::write(d.path().join("a.txt"), "abc").unwrap();
        fs::write(d.path().join("sub").join("b.txt"), "de").unwrap();
        assert_eq!(
            sha256_file(&d.path().join("a.txt")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(walk_files(d.path()).len(), 2);
        assert_eq!(dir_size(d.path()), 5);
    }
}
```

- [ ] **Step 2: Write the failing detect tests** — bottom of `detect.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PNG: [u8; 16] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R'];
    const MP4: [u8; 16] = [0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 2, 0];

    #[test]
    fn canonical_formats() {
        assert_eq!(canonical("JPG"), "jpeg");
        assert_eq!(canonical("tif"), "tiff");
        assert_eq!(canonical("htm"), "html");
        assert_eq!(canonical("markdown"), "markdown");
        assert_eq!(canonical("md"), "markdown");
        assert_eq!(canonical("aif"), "aiff");
        assert_eq!(canonical("heic"), "heif");
        assert_eq!(canonical("mkv"), "mkv");
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_for_ext("MOV"), Kind::Video);
        assert_eq!(kind_for_ext("opus"), Kind::Audio);
        assert_eq!(kind_for_ext("jxl"), Kind::Image);
        assert_eq!(kind_for_ext("pdf"), Kind::Pdf);
        assert_eq!(kind_for_ext("docx"), Kind::Text);
        assert_eq!(kind_for_ext("exe"), Kind::Unknown);
    }

    #[test]
    fn magic_bytes_correct_a_misnamed_image() {
        assert_eq!(classify(Path::new("C:/x/photo.jpg"), &PNG), (Kind::Image, "png".to_string()));
    }

    #[test]
    fn extension_wins_between_audio_and_video_containers() {
        assert_eq!(classify(Path::new("C:/x/song.m4a"), &MP4), (Kind::Audio, "m4a".to_string()));
        assert_eq!(classify(Path::new("C:/x/clip.mp4"), &MP4), (Kind::Video, "mp4".to_string()));
    }

    #[test]
    fn magic_bytes_rescue_an_unknown_extension() {
        assert_eq!(classify(Path::new("C:/x/download.bin"), &PNG), (Kind::Image, "png".to_string()));
    }

    #[test]
    fn parses_ffprobe_json_ignoring_cover_art() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}},
            {"codec_type":"audio","codec_name":"alac","disposition":{"attached_pic":0}}],
            "format":{"duration":"134.5"}}"#;
        let p = parse_ffprobe_json(json).unwrap();
        assert_eq!(p, Probe { duration_s: Some(134.5), has_audio: true, has_video: false, audio_codec: Some("alac".into()) });
    }

    #[test]
    fn refine_turns_audio_only_mov_into_audio_and_alac_m4a_into_alac() {
        let mut mov = FileInfo::bare(Path::new("C:/x/voice.mov"), Kind::Video, "mov".into(), 10);
        refine(&mut mov, &Probe { duration_s: Some(3.0), has_audio: true, has_video: false, audio_codec: Some("aac".into()) });
        assert_eq!(mov.kind, Kind::Audio);
        assert_eq!(mov.duration_s, Some(3.0));

        let mut m4a = FileInfo::bare(Path::new("C:/x/song.m4a"), Kind::Audio, "m4a".into(), 10);
        refine(&mut m4a, &Probe { duration_s: None, has_audio: true, has_video: false, audio_codec: Some("alac".into()) });
        assert_eq!(m4a.format, "alac");
        assert_eq!(m4a.has_audio, Some(true));
    }
}
```

Run: `cargo test detect::`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `detect.rs`** (above the tests)

```rust
//! What a file is (spec §6.1): kind + canonical format, refined by ffprobe.
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Audio,
    Image,
    Pdf,
    Text,
    Unknown,
}

const VIDEO: &[&str] = &["mp4", "mkv", "mov", "webm", "avi", "wmv", "flv", "m4v", "mpg", "ts", "3gp"];
const AUDIO: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma", "aiff", "aif", "alac"];
const IMAGE: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "gif", "ico"];
const TEXT: &[&str] = &["epub", "docx", "odt", "md", "markdown", "html", "htm", "txt", "rtf"];

pub fn kind_for_ext(ext: &str) -> Kind {
    let e = ext.to_ascii_lowercase();
    let e = e.as_str();
    if VIDEO.contains(&e) {
        Kind::Video
    } else if AUDIO.contains(&e) {
        Kind::Audio
    } else if IMAGE.contains(&e) {
        Kind::Image
    } else if e == "pdf" {
        Kind::Pdf
    } else if TEXT.contains(&e) {
        Kind::Text
    } else {
        Kind::Unknown
    }
}

/// Extension → canonical format (spec §5). Unlisted extensions map to themselves.
pub fn canonical(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpeg".into(),
        "tif" | "tiff" => "tiff".into(),
        "htm" | "html" => "html".into(),
        "md" | "markdown" => "markdown".into(),
        "aif" | "aiff" => "aiff".into(),
        "heic" | "heif" => "heif".into(),
        other => other.into(),
    }
}

pub fn ext_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

fn kind_for_mime(mime: &str) -> Kind {
    if mime.starts_with("video/") {
        Kind::Video
    } else if mime.starts_with("audio/") {
        Kind::Audio
    } else if mime.starts_with("image/") {
        Kind::Image
    } else if mime == "application/pdf" {
        Kind::Pdf
    } else {
        Kind::Unknown
    }
}

/// Kind and canonical format from the extension and the first bytes.
/// The extension wins unless the magic bytes say a different top-level kind;
/// audio vs video containers are left to ffprobe; images take the magic format.
pub fn classify(path: &Path, head: &[u8]) -> (Kind, String) {
    let ext = ext_of(path);
    let ext_kind = kind_for_ext(&ext);
    let magic = infer::get(head).map(|t| (kind_for_mime(t.mime_type()), canonical(t.extension())));
    match magic {
        Some((mk, mfmt)) if mk != Kind::Unknown => {
            let av = |k: Kind| matches!(k, Kind::Video | Kind::Audio);
            if ext_kind == Kind::Unknown || (ext_kind != mk && !(av(ext_kind) && av(mk))) {
                (mk, mfmt)
            } else if mk == Kind::Image {
                (Kind::Image, mfmt)
            } else {
                (ext_kind, canonical(&ext))
            }
        }
        _ => (ext_kind, canonical(&ext)),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probe {
    pub duration_s: Option<f64>,
    pub has_audio: bool,
    pub has_video: bool,
    pub audio_codec: Option<String>,
}

#[derive(Deserialize)]
struct JProbe {
    #[serde(default)]
    streams: Vec<JStream>,
    format: Option<JFormat>,
}
#[derive(Deserialize)]
struct JStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    disposition: Option<JDisposition>,
}
#[derive(Deserialize)]
struct JDisposition {
    #[serde(default)]
    attached_pic: i32,
}
#[derive(Deserialize)]
struct JFormat {
    duration: Option<String>,
}

/// Parse `ffprobe -v error -print_format json -show_format -show_streams`.
pub fn parse_ffprobe_json(s: &str) -> Option<Probe> {
    let j: JProbe = serde_json::from_str(s).ok()?;
    let mut p = Probe {
        duration_s: j.format.and_then(|f| f.duration).and_then(|d| d.parse().ok()),
        ..Default::default()
    };
    for st in j.streams {
        let cover = st.disposition.map(|d| d.attached_pic == 1).unwrap_or(false);
        match st.codec_type.as_deref() {
            Some("video") if !cover => p.has_video = true,
            Some("audio") => {
                p.has_audio = true;
                if p.audio_codec.is_none() {
                    p.audio_codec = st.codec_name;
                }
            }
            _ => {}
        }
    }
    Some(p)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub ext: String,
    /// Canonical format (spec §5), e.g. "jpeg", "alac".
    pub format: String,
    pub size: u64,
    pub duration_s: Option<f64>,
    pub has_audio: Option<bool>,
    pub pages: Option<u32>,
}

impl FileInfo {
    pub fn bare(path: &Path, kind: Kind, format: String, size: u64) -> Self {
        Self {
            path: path.to_path_buf(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            kind,
            ext: ext_of(path),
            format,
            size,
            duration_s: None,
            has_audio: None,
            pages: None,
        }
    }
}

/// Apply ffprobe facts: audio-only video containers become Audio, ALAC m4a becomes "alac".
pub fn refine(info: &mut FileInfo, p: &Probe) {
    info.duration_s = p.duration_s;
    info.has_audio = Some(p.has_audio);
    if info.kind == Kind::Video && !p.has_video && p.has_audio {
        info.kind = Kind::Audio;
    }
    if info.format == "m4a" && p.audio_codec.as_deref() == Some("alac") {
        info.format = "alac".into();
    }
}

/// Detect one file. `probe` runs ffprobe when FFmpeg is installed (else returns None).
pub fn detect(path: &Path, probe: &dyn Fn(&Path) -> Option<Probe>) -> std::io::Result<FileInfo> {
    let meta = std::fs::metadata(path)?;
    let mut head = [0u8; 64];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    let (kind, format) = classify(path, &head[..n]);
    let mut info = FileInfo::bare(path, kind, format, meta.len());
    if matches!(kind, Kind::Video | Kind::Audio) {
        if let Some(p) = probe(path) {
            refine(&mut info, &p);
        }
    }
    Ok(info)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test detect:: fsutil::` (two filters: run `cargo test detect::` then `cargo test fsutil::`)
Expected: all pass. If `classify` for the MP4 bytes returns something unexpected, print `infer::get(&MP4)` in a scratch test: `infer` must recognise `ftypisom` as `video/mp4`. Adjust the fixture bytes (not the logic) if your `infer` version needs more header bytes.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(detect): file kinds, canonical formats and ffprobe parsing"
```

---
### Task 5: Preset model, validation and output naming

**Files:**
- Create: `src-tauri/src/presets/mod.rs`, `src-tauri/src/presets/naming.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod presets;`)

- [ ] **Step 1: Write the failing tests** — bottom of `presets/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    pub(crate) const MINI: &str = r#"
[[preset]]
id = "v-mp3"
label = "MP3"
description = "Audio only · MP3"
icon = "music"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "mp3"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-b:a', '{setting.quality.mp3_kbps}k', '{out}']]

[[preset]]
id = "pdf-merge"
label = "Merge"
description = "Merge into one PDF"
icon = "merge"
kinds = ["pdf"]
min_files = 2
engine = "qpdf"
bin = "qpdf"
out_ext = "pdf"
out_naming = "fixed:merged"
ok_exit = [0, 3]
passes = [['--empty', '--pages', '{inputs}', '--', '{out}']]
"#;

    fn engines() -> HashMap<String, Vec<String>> {
        HashMap::from([
            ("ffmpeg".to_string(), vec!["ffmpeg".to_string(), "ffprobe".to_string()]),
            ("qpdf".to_string(), vec!["qpdf".to_string()]),
        ])
    }

    fn keys() -> BTreeSet<String> {
        crate::settings::var_keys()
    }

    #[test]
    fn parses_and_validates() {
        let c = Catalog::parse(MINI).unwrap();
        c.validate(&engines(), &keys()).unwrap();
        let m = c.get("pdf-merge").unwrap();
        assert_eq!(m.out_naming, OutNaming::Fixed("merged".into()));
        assert_eq!(m.ok_exit, vec![0, 3]);
        assert_eq!(c.get("v-mp3").unwrap().ok_exit, vec![0]);
        assert_eq!(c.get("v-mp3").unwrap().progress, ProgressKind::Ffmpeg);
    }

    #[test]
    fn out_naming_parsing() {
        assert_eq!(OutNaming::try_from("replace_ext".to_string()).unwrap(), OutNaming::ReplaceExt);
        assert_eq!(OutNaming::try_from("suffix:compressed".to_string()).unwrap(), OutNaming::Suffix("compressed".into()));
        assert_eq!(OutNaming::try_from("folder:pages".to_string()).unwrap(), OutNaming::Folder("pages".into()));
        assert!(OutNaming::try_from("suffix:".to_string()).is_err());
        assert!(OutNaming::try_from("rename".to_string()).is_err());
    }

    #[test]
    fn validation_reports_every_problem() {
        let bad = MINI
            .replace("engine = \"qpdf\"", "engine = \"nope\"")
            .replace("{setting.quality.mp3_kbps}", "{setting.quality.nope}")
            .replace("id = \"pdf-merge\"", "id = \"v-mp3\"");
        let errs = Catalog::parse(&bad).unwrap().validate(&engines(), &keys()).unwrap_err();
        let all = errs.join("\n");
        assert!(all.contains("duplicate id"), "{all}");
        assert!(all.contains("unknown engine 'nope'"), "{all}");
        assert!(all.contains("unknown placeholder {setting.quality.nope}"), "{all}");
    }

    #[test]
    fn inputs_placeholder_requires_multi_file_preset() {
        let bad = MINI.replace("min_files = 2\n", "");
        let errs = Catalog::parse(&bad).unwrap().validate(&engines(), &keys()).unwrap_err();
        assert!(errs.join("\n").contains("{inputs} needs min_files >= 2"));
    }

    #[test]
    fn placeholder_scanner() {
        assert_eq!(placeholders("-sOutputFile={out_dir}\\page-%03d.png").unwrap(), vec!["out_dir"]);
        assert_eq!(placeholders("{in}[0]").unwrap(), vec!["in"]);
        assert!(placeholders("{in").is_err());
    }

    #[test]
    fn out_format_rules() {
        let c = Catalog::parse(MINI).unwrap();
        let p = c.get("v-mp3").unwrap();
        assert_eq!(p.out_format_for("mov"), "mp3");
        assert_eq!(p.out_ext_for(std::path::Path::new("C:/a/b.MOV")), "mp3");
    }
}
```

And at the bottom of `presets/naming.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    fn exists_in(set: &[&str]) -> impl Fn(&Path) -> bool {
        let s: HashSet<PathBuf> = set.iter().map(|n| Path::new("C:/d").join(n)).collect();
        move |p: &Path| s.contains(p)
    }

    #[test]
    fn replace_ext_and_collisions() {
        let src = Path::new("C:/d/holiday.mov");
        let none = exists_in(&[]);
        assert_eq!(final_path(Path::new("C:/d"), src, &OutNaming::ReplaceExt, "mp3", &none), Path::new("C:/d/holiday.mp3"));
        let taken = exists_in(&["holiday.mp3", "holiday (1).mp3"]);
        assert_eq!(final_path(Path::new("C:/d"), src, &OutNaming::ReplaceExt, "mp3", &taken), Path::new("C:/d/holiday (2).mp3"));
    }

    #[test]
    fn suffix_folder_and_fixed() {
        let src = Path::new("C:/d/holiday.mp4");
        let none = exists_in(&[]);
        assert_eq!(
            final_path(Path::new("C:/d"), src, &OutNaming::Suffix("compressed".into()), "mp4", &none),
            Path::new("C:/d/holiday (compressed).mp4")
        );
        assert_eq!(
            final_path(Path::new("C:/d"), Path::new("C:/d/report.pdf"), &OutNaming::Folder("pages".into()), "png", &none),
            Path::new("C:/d/report (pages)")
        );
        let taken = exists_in(&["merged.pdf"]);
        assert_eq!(
            final_path(Path::new("C:/d"), Path::new("C:/d/a.pdf"), &OutNaming::Fixed("merged".into()), "pdf", &taken),
            Path::new("C:/d/merged (1).pdf")
        );
    }
}
```

Run: `cargo test presets::`
Expected: FAIL to compile.

- [ ] **Step 2: Implement `presets/mod.rs`** (above the tests)

```rust
//! The preset catalog (spec §5): data in presets/presets.toml.
pub mod available;
pub mod command;
pub mod naming;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::detect::{canonical, ext_of, Kind};

/// Placeholders the job runner provides; `setting.*` comes from Settings::to_vars.
pub const BUILTIN_PLACEHOLDERS: &[&str] = &["in", "inputs", "in_dir", "out", "out_dir", "reader", "final_stem", "video_kbps"];

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Ffmpeg,
    GhostscriptPages,
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutNaming {
    ReplaceExt,
    Suffix(String),
    Folder(String),
    Fixed(String),
}

impl OutNaming {
    pub fn is_folder(&self) -> bool {
        matches!(self, OutNaming::Folder(_))
    }
}

impl TryFrom<String> for OutNaming {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let nonempty = |t: &str| if t.trim().is_empty() { Err(format!("empty name in out_naming '{s}'")) } else { Ok(t.to_string()) };
        if s == "replace_ext" {
            Ok(OutNaming::ReplaceExt)
        } else if let Some(t) = s.strip_prefix("suffix:") {
            Ok(OutNaming::Suffix(nonempty(t)?))
        } else if let Some(t) = s.strip_prefix("folder:") {
            Ok(OutNaming::Folder(nonempty(t)?))
        } else if let Some(t) = s.strip_prefix("fixed:") {
            Ok(OutNaming::Fixed(nonempty(t)?))
        } else {
            Err(format!("unknown out_naming '{s}'"))
        }
    }
}

impl<'de> Deserialize<'de> for OutNaming {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        OutNaming::try_from(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

fn one() -> usize {
    1
}
fn ok_exit_default() -> Vec<i32> {
    vec![0]
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub kinds: Vec<Kind>,
    #[serde(default = "one")]
    pub min_files: usize,
    /// Hidden when several files are selected (e.g. single image → PDF, replaced by Combine).
    #[serde(default)]
    pub hide_when_multi: bool,
    /// Engine id from engines.toml, and the logical bin to run.
    pub engine: String,
    pub bin: String,
    /// One argv template per pass (Shrink has two).
    pub passes: Vec<Vec<String>>,
    /// Output extension, or "same" for the input's extension.
    pub out_ext: String,
    /// Canonical output format when it differs from canonical(out_ext) (ALAC writes .m4a).
    #[serde(default)]
    pub out_format: Option<String>,
    pub out_naming: OutNaming,
    #[serde(default)]
    pub progress: ProgressKind,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Shrink/Compress: shown even when the output format equals the input's.
    #[serde(default)]
    pub keep_same_format: bool,
    /// Compress: if the result isn't smaller, discard it and report ALREADY_SMALL.
    #[serde(default)]
    pub discard_if_not_smaller: bool,
    /// Hard-link inputs into the temp dir as in-N.<ext> (ImageMagick, Ghostscript).
    #[serde(default)]
    pub stage_inputs: bool,
    /// Extra names the engine writes into the temp dir, moved next to the output.
    #[serde(default)]
    pub extra_outputs: Vec<String>,
    /// Exit codes that mean success.
    #[serde(default = "ok_exit_default")]
    pub ok_exit: Vec<i32>,
}

impl Preset {
    pub fn is_multi(&self) -> bool {
        self.min_files >= 2
    }

    pub fn out_ext_for(&self, input: &Path) -> String {
        if self.out_ext == "same" {
            let e = ext_of(input);
            if e.is_empty() { "bin".into() } else { e }
        } else {
            self.out_ext.clone()
        }
    }

    pub fn out_format_for(&self, input_format: &str) -> String {
        match (&self.out_format, self.out_ext.as_str()) {
            (Some(f), _) => f.clone(),
            (None, "same") => input_format.to_string(),
            (None, ext) => canonical(ext),
        }
    }

    /// Does any pass use this placeholder?
    pub fn uses(&self, key: &str) -> bool {
        self.passes.iter().flatten().any(|a| placeholders(a).map(|k| k.iter().any(|p| p == key)).unwrap_or(false))
    }
}

/// Names inside `{...}` in one argument template.
pub fn placeholders(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or_else(|| format!("unclosed placeholder in '{s}'"))?;
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    Ok(out)
}

#[derive(Deserialize, Debug, Clone)]
pub struct Catalog {
    #[serde(rename = "preset")]
    pub presets: Vec<Preset>,
}

impl Catalog {
    pub fn parse(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("presets.toml: {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// `engines`: engine id → logical bin names (from the manifest).
    pub fn validate(&self, engines: &HashMap<String, Vec<String>>, setting_keys: &BTreeSet<String>) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        let mut seen = HashSet::new();
        for p in &self.presets {
            let id = &p.id;
            if !seen.insert(id.clone()) {
                errs.push(format!("{id}: duplicate id"));
            }
            match engines.get(&p.engine) {
                None => errs.push(format!("{id}: unknown engine '{}'", p.engine)),
                Some(bins) if !bins.contains(&p.bin) => errs.push(format!("{id}: engine '{}' has no bin '{}'", p.engine, p.bin)),
                _ => {}
            }
            if p.kinds.is_empty() {
                errs.push(format!("{id}: kinds is empty"));
            }
            if p.min_files == 0 {
                errs.push(format!("{id}: min_files must be >= 1"));
            }
            if p.ok_exit.is_empty() {
                errs.push(format!("{id}: ok_exit is empty"));
            }
            if p.passes.is_empty() || p.passes.iter().any(|a| a.is_empty()) {
                errs.push(format!("{id}: passes must be non-empty"));
            }
            let mut keys = Vec::new();
            for a in p.passes.iter().flatten().chain(&p.extra_outputs) {
                match placeholders(a) {
                    Ok(k) => keys.extend(k),
                    Err(e) => errs.push(format!("{id}: {e}")),
                }
            }
            for k in &keys {
                if !BUILTIN_PLACEHOLDERS.contains(&k.as_str()) && !setting_keys.contains(k) {
                    errs.push(format!("{id}: unknown placeholder {{{k}}}"));
                }
            }
            let has = |k: &str| keys.iter().any(|x| x == k);
            if has("inputs") && p.min_files < 2 {
                errs.push(format!("{id}: {{inputs}} needs min_files >= 2"));
            }
            if p.min_files >= 2 && !has("inputs") {
                errs.push(format!("{id}: a multi-file preset must use {{inputs}}"));
            }
            if p.out_naming.is_folder() != has("out_dir") || (!p.out_naming.is_folder() && !has("out")) {
                errs.push(format!("{id}: output placeholder doesn't match out_naming"));
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}
```

Create empty stubs so the module compiles: `presets/available.rs` and `presets/command.rs` each containing only `//! Filled in by a later task.`

- [ ] **Step 3: Implement `presets/naming.rs`** (above its tests)

```rust
//! Output names (spec §5.7): never overwrite, number on collision.
use std::path::{Path, PathBuf};

use super::OutNaming;

pub fn base_name(source: &Path, naming: &OutNaming) -> String {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "output".into());
    match naming {
        OutNaming::ReplaceExt => stem,
        OutNaming::Suffix(s) | OutNaming::Folder(s) => format!("{stem} ({s})"),
        OutNaming::Fixed(n) => n.clone(),
    }
}

/// `dir/base.ext`, else `dir/base (1).ext`, `dir/base (2).ext`, … (no ext for folders).
pub fn unique_path(dir: &Path, base: &str, ext: Option<&str>, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let name = |n: u32| {
        let b = if n == 0 { base.to_string() } else { format!("{base} ({n})") };
        match ext {
            Some(e) => format!("{b}.{e}"),
            None => b,
        }
    };
    let mut n = 0;
    loop {
        let p = dir.join(name(n));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

pub fn final_path(dir: &Path, source: &Path, naming: &OutNaming, ext: &str, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let ext = if naming.is_folder() { None } else { Some(ext) };
    unique_path(dir, &base_name(source, naming), ext, exists)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test presets::`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(presets): preset model, catalog validation and output naming"
```

---

### Task 6: Command builder

**Files:**
- Modify: `src-tauri/src/presets/command.rs`

- [ ] **Step 1: Write the failing tests** — `presets/command.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{tests::MINI, Catalog};
    use std::path::PathBuf;

    fn vars() -> BTreeMap<String, String> {
        crate::settings::Settings::default().to_vars()
    }

    fn argv(v: &[OsString]) -> Vec<String> {
        v.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn substitutes_and_prefixes_ffmpeg_flags() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from(r"C:\in\café 日本 %[1] -x.mov")];
        let v = vars();
        let passes = build_command(c.get("v-mp3").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "mp3", vars: &v }).unwrap();
        assert_eq!(
            argv(&passes[0]),
            ["-hide_banner", "-nostdin", "-progress", "pipe:1", "-nostats", "-i", r"C:\in\café 日本 %[1] -x.mov", "-b:a", "192k", "out.mp3"]
        );
    }

    #[test]
    fn inputs_expand_to_one_arg_each() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from(r"C:\in\a.pdf"), PathBuf::from(r"C:\in\b c.pdf")];
        let v = vars();
        let passes = build_command(c.get("pdf-merge").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "pdf", vars: &v }).unwrap();
        assert_eq!(argv(&passes[0]), ["--empty", "--pages", r"C:\in\a.pdf", r"C:\in\b c.pdf", "--", "out.pdf"]);
    }

    #[test]
    fn rejects_inputs_that_look_like_options() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from("-rf.mov")];
        let v = vars();
        let err = build_command(c.get("v-mp3").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "mp3", vars: &v }).unwrap_err();
        assert!(err.contains("starts with '-'"), "{err}");
    }

    #[test]
    fn unknown_placeholder_is_an_error() {
        let mut values = BTreeMap::new();
        values.insert("in".to_string(), "x".to_string());
        assert!(expand("{in}{nope}", &values).unwrap_err().contains("{nope}"));
        assert_eq!(expand("{in}[0]", &values).unwrap(), "x[0]");
    }
}
```

Note: the `tests` module in `presets/mod.rs` must be visible to siblings: change its declaration to `#[cfg(test)] pub(crate) mod tests {`.

Run: `cargo test presets::command`
Expected: FAIL to compile.

- [ ] **Step 2: Implement** (above the tests)

```rust
//! Placeholder expansion → argv (spec §6.2). Pure: no I/O.
//! Engines run with the job's temp dir as cwd, so {out} and {out_dir} are the
//! relative names "out.<ext>" and "out".
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::Preset;

pub struct BuildInput<'a> {
    /// Absolute originals, or relative staged names (in-1.png) for stage_inputs presets.
    pub inputs: &'a [PathBuf],
    /// Folder of the first original input (pandoc --resource-path).
    pub in_dir: &'a Path,
    pub out_ext: &'a str,
    /// setting.* plus job values: reader, final_stem, video_kbps.
    pub vars: &'a BTreeMap<String, String>,
}

const FFMPEG_PREFIX: &[&str] = &["-hide_banner", "-nostdin", "-progress", "pipe:1", "-nostats"];

pub fn expand(template: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or_else(|| format!("unclosed placeholder in '{template}'"))?;
        let key = &after[..end];
        out.push_str(values.get(key).ok_or_else(|| format!("unknown placeholder {{{key}}}"))?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn path_str(p: &Path) -> Result<String, String> {
    let s = p.to_str().ok_or_else(|| format!("path is not valid Unicode: {}", p.display()))?;
    if s.starts_with('-') {
        return Err(format!("input path starts with '-': {s}"));
    }
    Ok(s.to_string())
}

pub fn build_command(preset: &Preset, b: &BuildInput) -> Result<Vec<Vec<OsString>>, String> {
    let first = b.inputs.first().ok_or("no input files")?;
    let mut values = b.vars.clone();
    values.insert("in".into(), path_str(first)?);
    values.insert("in_dir".into(), path_str(b.in_dir)?);
    values.insert("out".into(), format!("out.{}", b.out_ext));
    values.insert("out_dir".into(), "out".into());
    let inputs: Vec<String> = b.inputs.iter().map(|p| path_str(p)).collect::<Result<_, _>>()?;

    let mut passes = Vec::new();
    for template in &preset.passes {
        let mut argv: Vec<OsString> = Vec::new();
        if preset.engine == "ffmpeg" {
            argv.extend(FFMPEG_PREFIX.iter().map(OsString::from));
        }
        for arg in template {
            if arg.contains("{inputs}") {
                for input in &inputs {
                    let mut v = values.clone();
                    v.insert("inputs".into(), input.clone());
                    argv.push(expand(arg, &v)?.into());
                }
            } else {
                argv.push(expand(arg, &values)?.into());
            }
        }
        passes.push(argv);
    }
    Ok(passes)
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test presets::`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(presets): argv builder with placeholder expansion"
```

---

### Task 7: The v1 catalog and its snapshot tests

**Files:**
- Create: `src-tauri/presets/presets.toml`
- Create: `src-tauri/src/presets/catalog_tests.rs`
- Modify: `src-tauri/src/presets/mod.rs` (add `pub const BUNDLED` and `#[cfg(test)] mod catalog_tests;`)

- [ ] **Step 1: Write `src-tauri/presets/presets.toml`**

```toml
# Recast preset catalog (spec §5). Ring order = order within each kind.
# Placeholders: {in} {inputs} {in_dir} {out} {out_dir} {reader} {final_stem} {video_kbps} {setting.<path>}
# Engines run with the job temp dir as cwd: {out} = "out.<ext>", {out_dir} = "out".

# ───────────── Video (§5.1) ─────────────
[[preset]]
id = "video-mp3"
label = "MP3"
description = "Audio only · MP3"
icon = "music"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "mp3"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'libmp3lame', '-b:a', '{setting.quality.mp3_kbps}k', '{out}']]

[[preset]]
id = "video-mp4"
label = "MP4"
description = "Plays everywhere · MP4"
icon = "film"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "mp4"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-c:v', 'libx264', '-crf', '23', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2', '-c:a', 'aac', '-b:a', '160k', '-movflags', '+faststart', '{out}']]

[[preset]]
id = "video-gif"
label = "GIF"
description = "Animated GIF"
icon = "sparkle"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "gif"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vf', 'fps={setting.gif.fps},scale={setting.gif.width}:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse', '-loop', '0', '{out}']]

[[preset]]
id = "video-webm"
label = "WebM"
description = "For the web · WebM"
icon = "globe"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "webm"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-c:v', 'libvpx-vp9', '-crf', '32', '-b:v', '0', '-row-mt', '1', '-c:a', 'libopus', '-b:a', '128k', '{out}']]

[[preset]]
id = "video-shrink"
label = "Shrink"
description = "Shrink to a size limit · MP4"
icon = "shrink"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
keep_same_format = true
out_ext = "mp4"
out_naming = "suffix:compressed"
passes = [
  ['-y', '-i', '{in}', '-c:v', 'libx264', '-preset', 'medium', '-b:v', '{video_kbps}k', '-pix_fmt', 'yuv420p', '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2', '-pass', '1', '-passlogfile', 'pass', '-an', '-f', 'null', 'NUL'],
  ['-i', '{in}', '-c:v', 'libx264', '-preset', 'medium', '-b:v', '{video_kbps}k', '-pix_fmt', 'yuv420p', '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2', '-pass', '2', '-passlogfile', 'pass', '-c:a', 'aac', '-b:a', '128k', '-movflags', '+faststart', '{out}'],
]

[[preset]]
id = "video-m4a"
label = "M4A"
description = "Audio only · M4A"
icon = "music"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "m4a"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'aac', '-b:a', '{setting.quality.aac_kbps}k', '{out}']]

[[preset]]
id = "video-mkv"
label = "MKV"
description = "Repackage as MKV (no re-encode)"
icon = "box"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "mkv"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-map', '0:v', '-map', '0:a?', '-c', 'copy', '{out}']]

[[preset]]
id = "video-mov"
label = "MOV"
description = "QuickTime · MOV"
icon = "film"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "mov"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-c:v', 'libx264', '-crf', '23', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2', '-c:a', 'aac', '-b:a', '160k', '{out}']]

[[preset]]
id = "video-opus"
label = "Opus"
description = "Audio only · Opus"
icon = "music"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "opus"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'libopus', '-b:a', '128k', '{out}']]

[[preset]]
id = "video-wav"
label = "WAV"
description = "Audio only · WAV"
icon = "wave"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "wav"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'pcm_s16le', '{out}']]

# ───────────── Audio (§5.2) ─────────────
[[preset]]
id = "audio-mp3"
label = "MP3"
description = "MP3"
icon = "music"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "mp3"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'libmp3lame', '-b:a', '{setting.quality.mp3_kbps}k', '{out}']]

[[preset]]
id = "audio-m4a"
label = "M4A"
description = "AAC · M4A"
icon = "music"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "m4a"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'aac', '-b:a', '{setting.quality.aac_kbps}k', '{out}']]

[[preset]]
id = "audio-opus"
label = "Opus"
description = "Opus"
icon = "music"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "opus"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'libopus', '-b:a', '128k', '{out}']]

[[preset]]
id = "audio-flac"
label = "FLAC"
description = "Lossless · FLAC"
icon = "wave"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "flac"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'flac', '{out}']]

[[preset]]
id = "audio-wav"
label = "WAV"
description = "Uncompressed · WAV"
icon = "wave"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "wav"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'pcm_s16le', '{out}']]

[[preset]]
id = "audio-alac"
label = "ALAC"
description = "Apple Lossless · M4A"
icon = "wave"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "m4a"
out_format = "alac"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'alac', '{out}']]

[[preset]]
id = "audio-aiff"
label = "AIFF"
description = "Uncompressed · AIFF"
icon = "wave"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "aiff"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'pcm_s16be', '{out}']]

[[preset]]
id = "audio-ogg"
label = "OGG"
description = "Ogg Vorbis"
icon = "music"
kinds = ["audio"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
out_ext = "ogg"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-vn', '-c:a', 'libvorbis', '-q:a', '5', '{out}']]

# ───────────── Images (§5.3) ─────────────
# Every single-image preset reads {in}[0] (first frame/page). Inputs are staged as in-N.<ext>.
[[preset]]
id = "image-jpg"
label = "JPG"
description = "JPEG photo"
icon = "image"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "jpg"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '-background', 'white', '-alpha', 'remove', '-alpha', 'off', '-quality', '{setting.quality.jpg_q}', '{out}']]

[[preset]]
id = "image-png"
label = "PNG"
description = "Lossless · PNG"
icon = "image"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "png"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '{out}']]

[[preset]]
id = "image-webp"
label = "WebP"
description = "Small for the web · WebP"
icon = "globe"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "webp"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '-quality', '{setting.quality.webp_q}', '{out}']]

[[preset]]
id = "image-avif"
label = "AVIF"
description = "Smallest · AVIF"
icon = "image"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "avif"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '-quality', '{setting.quality.avif_q}', '{out}']]

[[preset]]
id = "image-shrink"
label = "Shrink"
description = "Smaller, same format"
icon = "shrink"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
keep_same_format = true
out_ext = "same"
out_naming = "suffix:compressed"
passes = [['{in}[0]', '-auto-orient', '-resize', '{setting.shrink.image_px}x{setting.shrink.image_px}>', '-quality', '80', '{out}']]

[[preset]]
id = "image-jxl"
label = "JPEG XL"
description = "JPEG XL"
icon = "image"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "jxl"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '{out}']]

[[preset]]
id = "image-ico"
label = "ICO"
description = "Windows icon (16–256 px)"
icon = "grid"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "ico"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '-background', 'none', '-define', 'icon:auto-resize=256,48,32,16', '{out}']]

[[preset]]
id = "image-tiff"
label = "TIFF"
description = "TIFF"
icon = "image"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "tiff"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '{out}']]

[[preset]]
id = "image-pdf"
label = "PDF"
description = "Image as a PDF page"
icon = "doc"
kinds = ["image"]
engine = "imagemagick"
bin = "magick"
stage_inputs = true
hide_when_multi = true
out_ext = "pdf"
out_naming = "replace_ext"
passes = [['{in}[0]', '-auto-orient', '{out}']]

[[preset]]
id = "image-combine-pdf"
label = "Combine"
description = "Combine into one PDF"
icon = "stack"
kinds = ["image"]
min_files = 2
engine = "imagemagick"
bin = "magick"
stage_inputs = true
out_ext = "pdf"
out_naming = "fixed:combined"
passes = [['{inputs}[0]', '-auto-orient', '{out}']]

# ───────────── PDF (§5.4) ─────────────
[[preset]]
id = "pdf-compress"
label = "Compress"
description = "Smaller PDF"
icon = "shrink"
kinds = ["pdf"]
engine = "ghostscript"
bin = "gswin64c"
progress = "ghostscript_pages"
stage_inputs = true
keep_same_format = true
discard_if_not_smaller = true
out_ext = "pdf"
out_naming = "suffix:compressed"
passes = [['-dNOPAUSE', '-dBATCH', '-dSAFER', '-sDEVICE=pdfwrite', '-dPDFSETTINGS=/ebook', '-dCompatibilityLevel=1.5', '-sOutputFile={out}', '{in}']]

[[preset]]
id = "pdf-images"
label = "Images"
description = "Each page as a PNG"
icon = "image"
kinds = ["pdf"]
engine = "ghostscript"
bin = "gswin64c"
progress = "ghostscript_pages"
stage_inputs = true
out_ext = "png"
out_naming = "folder:pages"
passes = [['-dNOPAUSE', '-dBATCH', '-dSAFER', '-sDEVICE=png16m', '-r150', '-sOutputFile={out_dir}\page-%03d.png', '{in}']]

[[preset]]
id = "pdf-split"
label = "Split"
description = "One PDF per page"
icon = "split"
kinds = ["pdf"]
engine = "qpdf"
bin = "qpdf"
ok_exit = [0, 3]
keep_same_format = true
out_ext = "pdf"
out_naming = "folder:pages"
passes = [['{in}', '--split-pages', '{out_dir}\page-%d.pdf']]

[[preset]]
id = "pdf-merge"
label = "Merge"
description = "Merge into one PDF"
icon = "stack"
kinds = ["pdf"]
min_files = 2
engine = "qpdf"
bin = "qpdf"
ok_exit = [0, 3]
out_ext = "pdf"
out_naming = "fixed:merged"
passes = [['--empty', '--pages', '{inputs}', '--', '{out}']]

# ───────────── Ebooks / text (§5.5) ─────────────
[[preset]]
id = "text-epub"
label = "EPUB"
description = "E-book · EPUB"
icon = "book"
kinds = ["text"]
engine = "pandoc"
bin = "pandoc"
out_ext = "epub"
out_naming = "replace_ext"
passes = [['-f', '{reader}', '-t', 'epub', '--resource-path={in_dir}', '{in}', '-o', '{out}']]

[[preset]]
id = "text-docx"
label = "DOCX"
description = "Word document"
icon = "doc"
kinds = ["text"]
engine = "pandoc"
bin = "pandoc"
out_ext = "docx"
out_naming = "replace_ext"
passes = [['-f', '{reader}', '-t', 'docx', '--resource-path={in_dir}', '{in}', '-o', '{out}']]

[[preset]]
id = "text-html"
label = "HTML"
description = "Single web page"
icon = "globe"
kinds = ["text"]
engine = "pandoc"
bin = "pandoc"
out_ext = "html"
out_naming = "replace_ext"
passes = [['-f', '{reader}', '-t', 'html', '--standalone', '--embed-resources', '--resource-path={in_dir}', '{in}', '-o', '{out}']]

[[preset]]
id = "text-md"
label = "Markdown"
description = "Markdown (images beside it)"
icon = "text"
kinds = ["text"]
engine = "pandoc"
bin = "pandoc"
out_ext = "md"
out_naming = "replace_ext"
extra_outputs = ['{final_stem}_media']
passes = [['-f', '{reader}', '-t', 'gfm', '--extract-media={final_stem}_media', '--resource-path={in_dir}', '{in}', '-o', '{out}']]

[[preset]]
id = "text-odt"
label = "ODT"
description = "OpenDocument text"
icon = "doc"
kinds = ["text"]
engine = "pandoc"
bin = "pandoc"
out_ext = "odt"
out_naming = "replace_ext"
passes = [['-f', '{reader}', '-t', 'odt', '--resource-path={in_dir}', '{in}', '-o', '{out}']]
```

- [ ] **Step 2: Add the bundled constant** to `presets/mod.rs` (below the `use` lines)

```rust
pub const BUNDLED: &str = include_str!("../../presets/presets.toml");

/// Pandoc reader for an input extension (spec §5.5).
pub fn pandoc_reader(ext: &str) -> String {
    match ext {
        "txt" | "md" | "markdown" => "markdown",
        "htm" | "html" => "html",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod catalog_tests;
```

- [ ] **Step 3: Write `presets/catalog_tests.rs`** (snapshot every preset's argv; ring order)

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::command::{build_command, BuildInput};
use super::*;

fn sample_ext(p: &Preset) -> &'static str {
    match p.kinds[0] {
        Kind::Video => "mov",
        Kind::Audio => "flac",
        Kind::Image => "png",
        Kind::Pdf => "pdf",
        Kind::Text => "md",
        Kind::Unknown => "bin",
    }
}

/// Engine bins as the manifest will declare them (cross-checked in Task 9).
pub(crate) fn engine_bins() -> HashMap<String, Vec<String>> {
    HashMap::from([
        ("ffmpeg".into(), vec!["ffmpeg".into(), "ffprobe".into()]),
        ("imagemagick".into(), vec!["magick".into()]),
        ("pandoc".into(), vec!["pandoc".into()]),
        ("qpdf".into(), vec!["qpdf".into()]),
        ("ghostscript".into(), vec!["gswin64c".into()]),
    ])
}

#[test]
fn bundled_catalog_is_valid() {
    let c = Catalog::parse(BUNDLED).unwrap();
    c.validate(&engine_bins(), &crate::settings::var_keys()).unwrap();
}

#[test]
fn video_ring_order_matches_spec() {
    let c = Catalog::parse(BUNDLED).unwrap();
    let ids: Vec<&str> = c.presets.iter().filter(|p| p.kinds.contains(&Kind::Video)).map(|p| p.label.as_str()).collect();
    assert_eq!(ids, ["MP3", "MP4", "GIF", "WebM", "Shrink", "M4A", "MKV", "MOV", "Opus", "WAV"]);
}

#[test]
fn every_preset_argv_snapshot() {
    let c = Catalog::parse(BUNDLED).unwrap();
    let mut vars = crate::settings::Settings::default().to_vars();
    vars.insert("reader".into(), "markdown".into());
    vars.insert("final_stem".into(), "café 日本 %[1] -x".into());
    vars.insert("video_kbps".into(), "1906".into());
    for p in &c.presets {
        let ext = sample_ext(p);
        let n = p.min_files;
        let inputs: Vec<PathBuf> = (1..=n)
            .map(|i| if p.stage_inputs { PathBuf::from(format!("in-{i}.{ext}")) } else { PathBuf::from(format!(r"C:\in\café 日本 %[1] -x{i}.{ext}")) })
            .collect();
        let out_ext = p.out_ext_for(Path::new(&format!("x.{ext}")));
        let passes = build_command(p, &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: &out_ext, vars: &vars }).unwrap();
        let argv: Vec<Vec<String>> = passes.iter().map(|a| a.iter().map(|s| s.to_string_lossy().into_owned()).collect()).collect();
        insta::assert_debug_snapshot!(p.id.clone(), argv);
    }
}
```

- [ ] **Step 4: Generate the snapshots and review them**

```bash
INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test presets::catalog_tests
cargo test presets::catalog_tests
```
Expected: first run writes `src-tauri/src/presets/snapshots/*.snap` (one per preset, 37 files); second run passes.
Now **read every `.snap` file** and check: ffmpeg presets start with the five prefix flags; ImageMagick ones start `in-1.png[0]`; Combine lists `in-1.png[0]`, `in-2.png[0]`; Merge lists both `C:\in\…` paths between `--pages` and `--`; Shrink has two passes using `1906k` and `-passlogfile pass`; `pdf-images` has `-sOutputFile=out\page-%03d.png`; `text-md` has `--extract-media=café 日本 %[1] -x_media`. Fix the TOML (not the snapshot) for anything wrong, then re-run Step 4.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(presets): v1 catalog with argv snapshots for every preset"
```

---

### Task 8: `available()` — which presets apply

**Files:**
- Modify: `src-tauri/src/presets/available.rs`

- [ ] **Step 1: Write the failing tests** — `presets/available.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{Catalog, BUNDLED};

    fn f(kind: Kind, format: &str, has_audio: Option<bool>) -> FileFacts {
        FileFacts { kind, format: format.into(), has_audio }
    }

    fn ids(a: &Availability) -> Vec<String> {
        a.presets.iter().map(|p| p.id.clone()).collect()
    }

    const NO_CAPS: Caps = Caps { magick: None };

    #[test]
    fn mov_with_audio_hides_only_mov() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = available(&c, &[f(Kind::Video, "mov", Some(true))], &NO_CAPS);
        assert_eq!(ids(&a), ["video-mp3", "video-mp4", "video-gif", "video-webm", "video-shrink", "video-m4a", "video-mkv", "video-opus", "video-wav"]);
        assert!(!a.open_in_recast);
    }

    #[test]
    fn mp4_keeps_shrink_but_hides_mp4() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Video, "mp4", None)], &NO_CAPS));
        assert!(!a.contains(&"video-mp4".to_string()));
        assert!(a.contains(&"video-shrink".to_string()));
    }

    #[test]
    fn silent_video_hides_audio_presets() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Video, "mp4", Some(false))], &NO_CAPS));
        for gone in ["video-mp3", "video-m4a", "video-opus", "video-wav"] {
            assert!(!a.contains(&gone.to_string()), "{gone}");
        }
    }

    #[test]
    fn alac_input_hides_alac_but_keeps_m4a() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Audio, "alac", Some(true))], &NO_CAPS));
        assert!(!a.contains(&"audio-alac".to_string()));
        assert!(a.contains(&"audio-m4a".to_string()));
    }

    #[test]
    fn two_pdfs_offer_merge_and_per_file_tools() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Pdf, "pdf", None), f(Kind::Pdf, "pdf", None)], &NO_CAPS));
        assert_eq!(a, ["pdf-compress", "pdf-images", "pdf-split", "pdf-merge"]);
        let one = ids(&available(&c, &[f(Kind::Pdf, "pdf", None)], &NO_CAPS));
        assert!(!one.contains(&"pdf-merge".to_string()));
    }

    #[test]
    fn several_images_swap_pdf_for_combine() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Image, "png", None), f(Kind::Image, "jpeg", None)], &NO_CAPS));
        assert!(a.contains(&"image-combine-pdf".to_string()));
        assert!(!a.contains(&"image-pdf".to_string()));
        assert!(a.contains(&"image-png".to_string()), "png→png is pointless but jpeg→png is not");
    }

    #[test]
    fn mixed_kinds_with_no_common_preset_open_in_recast() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = available(&c, &[f(Kind::Pdf, "pdf", None), f(Kind::Image, "png", None)], &NO_CAPS);
        assert!(a.presets.is_empty());
        assert!(a.open_in_recast);
    }

    #[test]
    fn magick_capabilities_hide_unwritable_formats() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let caps: BTreeSet<String> = ["magick:read:heic", "magick:read:png", "magick:write:png", "magick:write:jpeg"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let a = ids(&available(&c, &[f(Kind::Image, "heif", None)], &Caps { magick: Some(&caps) }));
        assert_eq!(a, ["image-jpg", "image-png"]);
    }
}
```

Run: `cargo test presets::available`
Expected: FAIL to compile.

- [ ] **Step 2: Implement** (above the tests)

```rust
//! Which presets apply to a set of files (spec §5, §5.6, §6.2).
use std::collections::BTreeSet;

use super::{Catalog, Preset};
use crate::detect::{FileInfo, Kind};

#[derive(Debug, Clone)]
pub struct FileFacts {
    pub kind: Kind,
    pub format: String,
    pub has_audio: Option<bool>,
}

impl From<&FileInfo> for FileFacts {
    fn from(i: &FileInfo) -> Self {
        Self { kind: i.kind, format: i.format.clone(), has_audio: i.has_audio }
    }
}

/// Installed-engine capabilities. `magick: None` = unknown (ImageMagick not installed): show everything.
pub struct Caps<'a> {
    pub magick: Option<&'a BTreeSet<String>>,
}

pub struct Availability<'a> {
    pub presets: Vec<&'a Preset>,
    /// Mixed kinds and nothing covers them all: offer "Open in Recast" (§5.6).
    pub open_in_recast: bool,
}

/// ImageMagick's name for a canonical format.
pub fn magick_name(format: &str) -> &str {
    if format == "heif" { "heic" } else { format }
}

fn applies(p: &Preset, files: &[FileFacts], caps: &Caps) -> bool {
    let n = files.len();
    if n == 0 || files.iter().any(|f| !p.kinds.contains(&f.kind)) {
        return false;
    }
    if n == 1 && p.is_multi() {
        return false;
    }
    if n > 1 && (n < p.min_files || p.hide_when_multi) {
        return false;
    }
    if !p.keep_same_format && !p.is_multi() && files.iter().all(|f| p.out_format_for(&f.format) == f.format) {
        return false;
    }
    if p.requires.iter().any(|r| r == "audio_stream") && files.iter().any(|f| f.has_audio == Some(false)) {
        return false;
    }
    if p.engine == "imagemagick" {
        if let Some(c) = caps.magick {
            for f in files {
                let out = p.out_format_for(&f.format);
                let read = format!("magick:read:{}", magick_name(&f.format));
                let write = format!("magick:write:{}", magick_name(&out));
                if !c.contains(&read) || !c.contains(&write) {
                    return false;
                }
            }
        }
    }
    true
}

pub fn available<'a>(catalog: &'a Catalog, files: &[FileFacts], caps: &Caps) -> Availability<'a> {
    let presets: Vec<&Preset> = catalog.presets.iter().filter(|p| applies(p, files, caps)).collect();
    let kinds: BTreeSet<Kind> = files.iter().map(|f| f.kind).collect();
    Availability { open_in_recast: presets.is_empty() && kinds.len() > 1, presets }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test presets::`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(presets): availability rules (same-format, audio, multi-file, magick caps)"
```

---
### Task 9: 7-Zip, the engine manifest and `refresh-engines`

**Files:**
- Create: `scripts/fetch-7zip.mjs`, `scripts/refresh-engines.mjs`
- Create (by the scripts): `src-tauri/bin/7z.exe`, `src-tauri/bin/7z.dll`, `src-tauri/bin/7zip-License.txt`, `src-tauri/engines/engines.toml`
- Create: `src-tauri/src/engines/mod.rs`, `src-tauri/src/engines/manifest.rs`
- Modify: `src-tauri/tauri.conf.json` (bundle resources), `src-tauri/src/lib.rs` (add `pub mod engines;`)

- [ ] **Step 1: Write `scripts/fetch-7zip.mjs`**

```js
// Downloads the current 7-Zip x64 release and extracts 7z.exe, 7z.dll and License.txt
// into src-tauri/bin/. 7zr.exe (the 7z-only standalone) opens the installer, which is a 7z SFX.
// Recast needs the full 7z.exe + 7z.dll: 7za.exe cannot unpack NSIS installers (Ghostscript).
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = join(root, ".cache", "7zip");
const out = join(root, "src-tauri", "bin");

async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  writeFileSync(dest, Buffer.from(await res.arrayBuffer()));
}

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
mkdirSync(out, { recursive: true });

const page = await (await fetch("https://www.7-zip.org/download.html")).text();
const m = page.match(/a\/7z(\d{4})-x64\.exe/);
if (!m) throw new Error("No x64 installer link on 7-zip.org/download.html");
const installer = join(work, `7z${m[1]}-x64.exe`);
await download(`https://www.7-zip.org/${m[0]}`, installer);
await download("https://www.7-zip.org/a/7zr.exe", join(work, "7zr.exe"));

execFileSync(join(work, "7zr.exe"), ["x", "-y", `-o${join(work, "x")}`, installer, "7z.exe", "7z.dll", "License.txt"], { stdio: "inherit" });
copyFileSync(join(work, "x", "7z.exe"), join(out, "7z.exe"));
copyFileSync(join(work, "x", "7z.dll"), join(out, "7z.dll"));
copyFileSync(join(work, "x", "License.txt"), join(out, "7zip-License.txt"));

const formats = execFileSync(join(out, "7z.exe"), ["i"], { encoding: "utf8" });
if (!/\bNsis\b/i.test(formats)) throw new Error("7z.exe does not list the Nsis format");
for (const f of ["7z.exe", "7z.dll"]) {
  console.log(`${f}  sha256 ${createHash("sha256").update(readFileSync(join(out, f))).digest("hex")}`);
}
console.log(`7-Zip ${m[1]} → src-tauri/bin`);
```

- [ ] **Step 2: Run it**

```bash
node scripts/fetch-7zip.mjs
```
Expected: ends with `7-Zip NNNN → src-tauri/bin`; the three files exist. If 7zr can't open the installer, download `https://www.7-zip.org/a/7z<NNNN>-x64.msi` instead and extract it with `7zr x` the same way (the MSI holds `7z.exe`, `7z.dll` and `License.txt` as `_7z.exe`-style names; rename them).

- [ ] **Step 3: Write `scripts/refresh-engines.mjs`**

```js
// Pins the latest engine releases into src-tauri/engines/engines.toml (spec §6.3):
// URL, sha256 and size per engine, then runs the capability checks before pinning.
// Usage: node scripts/refresh-engines.mjs   (set GITHUB_TOKEN to avoid API rate limits)
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, mkdtempSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cache = join(root, ".cache", "engines");
const sevenZip = join(root, "src-tauri", "bin", "7z.exe");

const ENGINES = [
  { id: "ffmpeg", name: "FFmpeg", repo: "GyanD/codexffmpeg", asset: /^ffmpeg-[\d.]+-essentials_build\.zip$/,
    licence: "GPL-3.0-or-later", homepage: "https://ffmpeg.org", source_url: "https://github.com/GyanD/codexffmpeg",
    bins: { ffmpeg: "ffmpeg.exe", ffprobe: "ffprobe.exe" } },
  { id: "imagemagick", name: "ImageMagick", repo: "ImageMagick/ImageMagick", asset: /portable-Q16-HDRI-x64\.(zip|7z)$/,
    licence: "ImageMagick", homepage: "https://imagemagick.org", source_url: "https://github.com/ImageMagick/ImageMagick",
    bins: { magick: "magick.exe" }, capabilities: "magick_formats" },
  { id: "pandoc", name: "Pandoc", repo: "jgm/pandoc", asset: /windows-x86_64\.zip$/,
    licence: "GPL-2.0-or-later", homepage: "https://pandoc.org", source_url: "https://github.com/jgm/pandoc",
    bins: { pandoc: "pandoc.exe" } },
  { id: "qpdf", name: "qpdf", repo: "qpdf/qpdf", asset: /msvc64\.zip$/,
    licence: "Apache-2.0", homepage: "https://qpdf.sourceforge.io", source_url: "https://github.com/qpdf/qpdf",
    bins: { qpdf: "qpdf.exe" } },
  { id: "ghostscript", name: "Ghostscript", repo: "ArtifexSoftware/ghostpdl-downloads", asset: /^gs\d+w64\.exe$/, archive: "nsis",
    licence: "AGPL-3.0-or-later", homepage: "https://ghostscript.com", source_url: "https://github.com/ArtifexSoftware/ghostpdl",
    bins: { gswin64c: "gswin64c.exe" }, check: ["gswin64c", "-v"] },
];

const headers = { "User-Agent": "recast-refresh-engines", Accept: "application/vnd.github+json",
  ...(process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}) };

async function latest(repo) {
  const r = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, { headers });
  if (!r.ok) throw new Error(`${repo}: GitHub API HTTP ${r.status}`);
  return r.json();
}

async function download(url, dest) {
  if (existsSync(dest)) return;
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(dest + ".part"));
  renameSync(dest + ".part", dest);
}

async function sha256(file) {
  const h = createHash("sha256");
  await pipeline(createReadStream(file), h);
  return h.digest("hex");
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]);
}
const find = (dir, name) => walk(dir).find((p) => basename(p).toLowerCase() === name.toLowerCase());

mkdirSync(cache, { recursive: true });
const pinned = [];
for (const e of ENGINES) {
  const rel = await latest(e.repo);
  const asset = rel.assets.find((a) => e.asset.test(a.name));
  if (!asset) throw new Error(`${e.id}: no asset matching ${e.asset} in ${rel.tag_name}`);
  const file = join(cache, asset.name);
  console.log(`${e.id}: ${asset.name}`);
  await download(asset.browser_download_url, file);
  const x = join(cache, "x", e.id);
  rmSync(x, { recursive: true, force: true });
  execFileSync(sevenZip, ["x", "-y", `-o${x}`, file], { stdio: "ignore" });
  for (const exe of Object.values(e.bins)) if (!find(x, exe)) throw new Error(`${e.id}: ${exe} not found after extraction`);
  pinned.push({ ...e, version: rel.tag_name.replace(/^v/, ""), url: asset.browser_download_url,
    sha256: await sha256(file), size: statSync(file).size,
    archive: e.archive ?? (asset.name.endsWith(".7z") ? "7z" : "zip") });
}

// Capability checks (spec §6.3): fail before pinning a build that lacks what the presets need.
const magick = find(join(cache, "x", "imagemagick"), "magick.exe");
const formats = execFileSync(magick, ["-list", "format"], { encoding: "utf8" }).split(/\r?\n/);
for (const [fmt, need] of [["HEIC", "r"], ["AVIF", "rw"], ["JXL", "rw"], ["WEBP", "rw"]]) {
  const t = formats.map((l) => l.trim().split(/\s+/)).find((t) => t[0]?.replace("*", "").toUpperCase() === fmt);
  if (!t || (need.includes("r") && t[2][0] !== "r") || (need.includes("w") && t[2][1] !== "w"))
    throw new Error(`ImageMagick build lacks ${fmt} ${need}`);
}
const gs = find(join(cache, "x", "ghostscript"), "gswin64c.exe");
const probe = mkdtempSync(join(tmpdir(), "recast-gs-"));
execFileSync(magick, ["-size", "100x100", "xc:white", join(probe, "t.pdf")]);
execFileSync(gs, ["-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=png16m", "-sOutputFile=out.png", join(probe, "t.pdf")], { cwd: probe, stdio: "ignore" });
if (!existsSync(join(probe, "out.png"))) throw new Error("gswin64c failed to run from an unpacked folder");

const q = (s) => JSON.stringify(s);
let toml = "# Generated by scripts/refresh-engines.mjs. Do not edit by hand.\n";
for (const e of pinned) {
  toml += `\n[[engine]]\nid = ${q(e.id)}\nname = ${q(e.name)}\nversion = ${q(e.version)}\nplatform = "windows-x64"\n`;
  toml += `url = ${q(e.url)}\nsha256 = ${q(e.sha256)}\nsize = ${e.size}\narchive = ${q(e.archive)}\n`;
  toml += `licence = ${q(e.licence)}\nhomepage = ${q(e.homepage)}\nsource_url = ${q(e.source_url)}\n`;
  if (e.check) toml += `check = [${e.check.map(q).join(", ")}]\n`;
  if (e.capabilities) toml += `capabilities = ${q(e.capabilities)}\n`;
  toml += `\n[engine.bins]\n${Object.entries(e.bins).map(([k, v]) => `${k} = ${q(v)}`).join("\n")}\n`;
}
mkdirSync(join(root, "src-tauri", "engines"), { recursive: true });
writeFileSync(join(root, "src-tauri", "engines", "engines.toml"), toml);
console.log("Wrote src-tauri/engines/engines.toml");
```

- [ ] **Step 4: Run it**

```bash
node scripts/refresh-engines.mjs
```
Expected: one line per engine, then `Wrote src-tauri/engines/engines.toml`. Open the file and check: five `[[engine]]` blocks, every `url` is `https://github.com/...`, every `sha256` is 64 hex chars. If an asset regex finds nothing (an upstream renamed its files), fix the regex, not the check.

- [ ] **Step 5: Bundle the 7-Zip files** — in `tauri.conf.json`, add inside `"bundle"`:

```json
"resources": {
  "bin/7z.exe": "bin/7z.exe",
  "bin/7z.dll": "bin/7z.dll",
  "bin/7zip-License.txt": "bin/7zip-License.txt"
},
```

- [ ] **Step 6: Write the failing manifest tests** — bottom of `engines/manifest.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[[engine]]
id = "qpdf"
name = "qpdf"
version = "12.0.0"
platform = "windows-x64"
url = "https://github.com/qpdf/qpdf/releases/download/v12.0.0/qpdf-12.0.0-msvc64.zip"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
size = 10
archive = "zip"
licence = "Apache-2.0"
homepage = "https://qpdf.sourceforge.io"

[engine.bins]
qpdf = "qpdf.exe"
"#;

    #[test]
    fn parses_and_validates() {
        let m = Manifest::parse(GOOD).unwrap();
        m.validate().unwrap();
        assert_eq!(m.get("qpdf").unwrap().bins["qpdf"], "qpdf.exe");
        assert_eq!(m.bins_map()["qpdf"], vec!["qpdf".to_string()]);
    }

    #[test]
    fn rejects_http_bad_hash_and_unknown_archive() {
        let bad = GOOD.replace("https://", "http://").replace("\"0000000000000000000000000000000000000000000000000000000000000000\"", "\"abc\"").replace("\"zip\"", "\"rar\"");
        let errs = Manifest::parse(&bad).unwrap().validate().unwrap_err().join("\n");
        assert!(errs.contains("https"), "{errs}");
        assert!(errs.contains("sha256"), "{errs}");
        assert!(errs.contains("archive"), "{errs}");
    }

    #[test]
    fn bundled_manifest_is_valid_and_covers_the_catalog() {
        let m = Manifest::parse(BUNDLED).unwrap();
        m.validate().unwrap();
        let c = crate::presets::Catalog::parse(crate::presets::BUNDLED).unwrap();
        c.validate(&m.bins_map(), &crate::settings::var_keys()).unwrap();
    }
}
```

Run: `cargo test engines::manifest`
Expected: FAIL to compile.

- [ ] **Step 7: Implement `engines/manifest.rs`** (above the tests) and the module file

```rust
//! The bundled engine manifest (spec §6.3). Updated only by app releases.
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub const BUNDLED: &str = include_str!("../../engines/engines.toml");

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct EngineEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub platform: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// "zip" | "7z" | "nsis" — all unpacked with the bundled 7z.exe.
    pub archive: String,
    pub licence: String,
    pub homepage: String,
    #[serde(default)]
    pub source_url: Option<String>,
    /// Logical bin name → exe file name, located anywhere in the unpacked tree.
    pub bins: BTreeMap<String, String>,
    /// Post-install smoke test: [bin, args…]; must exit 0.
    #[serde(default)]
    pub check: Vec<String>,
    /// "magick_formats": record `magick -list format` after install.
    #[serde(default)]
    pub capabilities: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Manifest {
    #[serde(rename = "engine")]
    pub engines: Vec<EngineEntry>,
}

impl Manifest {
    pub fn parse(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("engines.toml: {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&EngineEntry> {
        self.engines.iter().find(|e| e.id == id)
    }

    pub fn bins_map(&self) -> HashMap<String, Vec<String>> {
        self.engines.iter().map(|e| (e.id.clone(), e.bins.keys().cloned().collect())).collect()
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        let mut seen = HashSet::new();
        for e in &self.engines {
            let id = &e.id;
            if !seen.insert(id.clone()) {
                errs.push(format!("{id}: duplicate id"));
            }
            if e.platform != "windows-x64" {
                errs.push(format!("{id}: platform must be windows-x64"));
            }
            if !e.url.starts_with("https://") {
                errs.push(format!("{id}: url must be https"));
            }
            if e.sha256.len() != 64 || !e.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                errs.push(format!("{id}: sha256 must be 64 lowercase hex chars"));
            }
            if e.size == 0 {
                errs.push(format!("{id}: size must be > 0"));
            }
            if !["zip", "7z", "nsis"].contains(&e.archive.as_str()) {
                errs.push(format!("{id}: unknown archive '{}'", e.archive));
            }
            if e.bins.is_empty() {
                errs.push(format!("{id}: bins is empty"));
            }
            if let Some(b) = e.check.first() {
                if !e.bins.contains_key(b) {
                    errs.push(format!("{id}: check uses unknown bin '{b}'"));
                }
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}
```

`engines/mod.rs` (for now):
```rust
//! Engines: download on demand, verify, locate (spec §6.3).
pub mod manifest;
```

- [ ] **Step 8: Run the tests**

Run: `cargo test engines::manifest`
Expected: 3 passed.

- [ ] **Step 9: Commit** (the 7-Zip binaries are committed: they ship inside the app)

```bash
git add -A
git commit -m "feat(engines): bundled 7-Zip, pinned engine manifest and refresh script"
```

---

### Task 10: Engine install, status, verify, capabilities

**Files:**
- Create: `src-tauri/src/proc.rs`, `src-tauri/src/engines/install.rs`, `src-tauri/src/engines/caps.rs`
- Modify: `src-tauri/src/engines/mod.rs`, `src-tauri/src/lib.rs` (add `pub mod proc;`)

- [ ] **Step 1: Write `proc.rs` with its tests** (a bounded "run and capture stdout", used for ffprobe, qpdf page counts, `magick -list format` and install checks)

```rust
//! Short helper processes with a timeout (never a shell).
use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform;

/// Run `program args…`; return stdout if it exits 0 within `timeout`, else None.
pub fn run_capture<I, S>(program: &Path, args: I, timeout: Duration) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let group = platform::KillGroup::new().ok()?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(platform::CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let _ = group.assign(&child);
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        if let Ok(Some(st)) = child.try_wait() {
            break Some(st);
        }
        if start.elapsed() > timeout {
            group.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    group.kill();
    let out = reader.join().ok()?;
    match status {
        Some(st) if st.success() => Some(String::from_utf8_lossy(&out).into_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout() {
        let out = run_capture(Path::new("cmd"), ["/c", "echo hello"], Duration::from_secs(10)).unwrap();
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn times_out() {
        let t = Instant::now();
        assert!(run_capture(Path::new("ping"), ["-n", "20", "127.0.0.1"], Duration::from_millis(300)).is_none());
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn nonzero_exit_is_none() {
        assert!(run_capture(Path::new("cmd"), ["/c", "exit 2"], Duration::from_secs(10)).is_none());
    }
}
```

Run: `cargo test proc::`
Expected: 3 passed.

- [ ] **Step 2: Write `engines/caps.rs` test-first**

```rust
//! Parse `magick -list format` into "magick:read:<fmt>" / "magick:write:<fmt>" (spec §6.3).
use std::collections::BTreeSet;

pub fn parse_magick_formats(out: &str) -> BTreeSet<String> {
    let mut caps = BTreeSet::new();
    let mut in_table = false;
    for line in out.lines() {
        if line.trim_start().starts_with("---") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 3 {
            continue;
        }
        let mode = t[2].as_bytes();
        if mode.len() != 3 || !mode.iter().all(|c| matches!(c, b'r' | b'w' | b'+' | b'-')) {
            continue;
        }
        let fmt = t[0].trim_end_matches('*').to_ascii_lowercase();
        if mode[0] == b'r' {
            caps.insert(format!("magick:read:{fmt}"));
        }
        if mode[1] == b'w' {
            caps.insert(format!("magick:write:{fmt}"));
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "   Format  Module    Mode  Description
-------------------------------------------------------------------------------
      3FR  DNG       r--   Hasselblad CFV/H3D39II Raw Format (0.22.0-Release)
     AVIF  HEIC      rw+   AV1 Image File Format (1.17.6)
     HEIC  HEIC      r--   High Efficiency Image Format (1.17.6)
      JXL* JXL       rw-   JPEG XL (ISO/IEC 18181) (libjxl 0.10.3)
                           continuation of a description line
* native blob support
";

    #[test]
    fn parses_modes() {
        let c = parse_magick_formats(SAMPLE);
        assert!(c.contains("magick:read:avif") && c.contains("magick:write:avif"));
        assert!(c.contains("magick:read:heic") && !c.contains("magick:write:heic"));
        assert!(c.contains("magick:write:jxl"));
        assert!(c.contains("magick:read:3fr"));
        assert_eq!(c.len(), 6);
    }
}
```

Run: `cargo test engines::caps` (after adding `pub mod caps;` to `engines/mod.rs`)
Expected: 1 passed.

- [ ] **Step 3: Write the failing install tests** — bottom of `engines/install.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::manifest::EngineEntry;
    use std::collections::BTreeMap;

    pub(crate) fn sevenzip() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join("7z.exe")
    }

    /// A zip holding tool/bin/tool.exe, built with the bundled 7z.exe.
    pub(crate) fn make_archive(dir: &Path) -> PathBuf {
        let src = dir.join("pkg").join("tool").join("bin");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("tool.exe"), b"not really an exe").unwrap();
        let zip = dir.join("tool-1.0.zip");
        let st = std::process::Command::new(sevenzip())
            .args(["a", "-tzip"])
            .arg(&zip)
            .arg(dir.join("pkg").join("*"))
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stdout));
        zip
    }

    pub(crate) fn entry(zip: &Path, version: &str) -> EngineEntry {
        EngineEntry {
            id: "tool".into(),
            name: "Tool".into(),
            version: version.into(),
            platform: "windows-x64".into(),
            url: "https://example.invalid/tool-1.0.zip".into(),
            sha256: crate::fsutil::sha256_file(zip).unwrap(),
            size: fs::metadata(zip).unwrap().len(),
            archive: "zip".into(),
            licence: "MIT".into(),
            homepage: "https://example.invalid".into(),
            source_url: None,
            bins: BTreeMap::from([("tool".to_string(), "tool.exe".to_string())]),
            check: vec![],
            capabilities: None,
        }
    }

    #[test]
    fn installs_locates_and_reports_status() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let e = entry(&zip, "1.0");
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip.clone() };
        let ctx = InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher };
        let mut seen = Vec::new();
        let rec = install(&e, &ctx, &mut |p| seen.push(p)).unwrap();
        assert_eq!(rec.bins["tool"].rel, r"tool\bin\tool.exe");
        assert!(root.join("tool").join("1.0").join(r"tool\bin\tool.exe").is_file());
        assert!(!root.join(".partial").join("tool").exists());
        assert!(seen.last().copied().unwrap_or(0.0) > 0.9);
        assert_eq!(read_status(&e, &root), EngineStatus::Installed { version: "1.0".into(), update_available: false });
        let newer = entry(&zip, "2.0");
        assert_eq!(read_status(&newer, &root), EngineStatus::Installed { version: "1.0".into(), update_available: true });
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let mut e = entry(&zip, "1.0");
        e.sha256 = "0".repeat(64);
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        let err = install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!root.join("tool").exists());
        assert!(crate::fsutil::walk_files(&root).is_empty(), "the bad download must be deleted");
        assert_eq!(read_status(&e, &root), EngineStatus::NotInstalled);
    }

    #[test]
    fn failing_post_install_check_installs_nothing() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let mut e = entry(&zip, "1.0");
        e.check = vec!["tool".into(), "-v".into()];
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        assert!(install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).is_err());
        assert!(!root.join("tool").exists());
    }

    #[test]
    fn verify_detects_tampering_and_remove_deletes() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let e = entry(&zip, "1.0");
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap();
        assert!(verify(&e, &root).unwrap());
        fs::write(root.join("tool").join("1.0").join(r"tool\bin\tool.exe"), b"tampered").unwrap();
        assert!(!verify(&e, &root).unwrap());
        assert_eq!(read_status(&e, &root), EngineStatus::Damaged);
        remove(&e, &root).unwrap();
        assert_eq!(read_status(&e, &root), EngineStatus::NotInstalled);
    }
}
```

Run: `cargo test engines::install`
Expected: FAIL to compile.

- [ ] **Step 4: Implement `engines/install.rs`** (above the tests)

```rust
//! Fetch → verify SHA-256 → unpack with 7z → locate bins → smoke check → atomic rename.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::manifest::EngineEntry;
use crate::fsutil::{sha256_file, walk_files};
use crate::platform;

pub const RECORD: &str = ".recast-install.json";
pub const DAMAGED: &str = ".recast-damaged";
pub const CAPS: &str = ".recast-caps.json";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineStatus {
    NotInstalled,
    Installing { pct: f32 },
    Installed { version: String, update_available: bool },
    Damaged,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BinRecord {
    pub rel: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstallRecord {
    pub id: String,
    pub version: String,
    pub bins: BTreeMap<String, BinRecord>,
}

pub trait Fetcher: Send + Sync {
    /// Download `url` into `dest`, resuming if `dest` already holds a prefix.
    fn fetch(&self, url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String>;
}

pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
        let have = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        let client = reqwest::blocking::Client::builder()
            .user_agent("Recast")
            .connect_timeout(Duration::from_secs(30))
            // Blocking reqwest applies this per send() and per body read(): an idle
            // timeout, not a cap on the whole download. A stalled connection errors out.
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status();
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(()); // already complete; the checksum decides
        }
        let (mut file, mut done) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            (OpenOptions::new().append(true).open(dest).map_err(|e| e.to_string())?, have)
        } else if status.is_success() {
            (File::create(dest).map_err(|e| e.to_string())?, 0)
        } else {
            return Err(format!("download failed: HTTP {status}"));
        };
        let total = resp.content_length().map(|l| l + done);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            progress(done, total);
        }
        Ok(())
    }
}

/// Copies a local file instead of downloading (tests, and the real-engine test cache).
pub struct LocalFetcher {
    pub source: PathBuf,
}

impl Fetcher for LocalFetcher {
    fn fetch(&self, _url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
        let n = fs::copy(&self.source, dest).map_err(|e| e.to_string())?;
        progress(n, Some(n));
        Ok(())
    }
}

pub struct InstallCtx<'a> {
    pub root: &'a Path,
    pub sevenzip: &'a Path,
    pub fetcher: &'a dyn Fetcher,
}

pub fn install(entry: &EngineEntry, ctx: &InstallCtx, progress: &mut dyn FnMut(f32)) -> Result<InstallRecord, String> {
    let work = ctx.root.join(".partial").join(&entry.id);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let file_name = entry.url.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("download");
    let archive = work.join(file_name);
    let staging = work.join("x");

    let result = (|| {
        let size = entry.size.max(1);
        ctx.fetcher.fetch(&entry.url, &archive, &mut |done, total| {
            progress((done as f32 / total.unwrap_or(size).max(1) as f32).min(1.0) * 0.85)
        })?;
        let digest = sha256_file(&archive).map_err(|e| e.to_string())?;
        if digest != entry.sha256 {
            let _ = fs::remove_file(&archive);
            return Err(format!("checksum mismatch for {} (the download was deleted)", entry.name));
        }
        let _ = fs::remove_dir_all(&staging);
        extract(ctx.sevenzip, &archive, &staging)?;
        progress(0.92);
        let record = locate_bins(entry, &staging)?;
        fs::write(staging.join(RECORD), serde_json::to_vec_pretty(&record).unwrap()).map_err(|e| e.to_string())?;
        if let Some((bin, args)) = entry.check.split_first() {
            let exe = staging.join(&record.bins[bin].rel);
            crate::proc::run_capture(&exe, args, Duration::from_secs(30))
                .ok_or_else(|| format!("{} didn't start after unpacking", entry.name))?;
        }
        let engine_dir = ctx.root.join(&entry.id);
        let final_dir = engine_dir.join(&entry.version);
        fs::create_dir_all(&engine_dir).map_err(|e| e.to_string())?;
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir).map_err(|e| e.to_string())?;
        }
        // Defender scans hundreds of fresh exe/dll files; retry the directory rename.
        crate::fsutil::rename_with_retry(&staging, &final_dir, Duration::from_secs(15)).map_err(|e| e.to_string())?;
        for old in fs::read_dir(&engine_dir).into_iter().flatten().flatten() {
            if old.path() != final_dir {
                let _ = fs::remove_dir_all(old.path());
            }
        }
        progress(1.0);
        Ok(record)
    })();

    let _ = fs::remove_dir_all(&staging);
    if result.is_ok() {
        let _ = fs::remove_dir_all(&work);
    }
    // On a network failure the partial archive stays in .partial so the next attempt resumes.
    result
}

fn extract(sevenzip: &Path, archive: &Path, dest: &Path) -> Result<(), String> {
    let out = Command::new(sevenzip)
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", dest.display()))
        .arg(archive)
        .creation_flags(platform::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("couldn't run 7-Zip: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("unpacking failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn locate_bins(entry: &EngineEntry, dir: &Path) -> Result<InstallRecord, String> {
    let files = walk_files(dir);
    let mut bins = BTreeMap::new();
    for (logical, exe) in &entry.bins {
        let found = files
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(exe)))
            .min_by_key(|p| p.components().count())
            .ok_or_else(|| format!("{exe} not found in the {} download", entry.name))?;
        let rel = found.strip_prefix(dir).unwrap().to_string_lossy().replace('/', "\\");
        bins.insert(logical.clone(), BinRecord { rel, sha256: sha256_file(found).map_err(|e| e.to_string())? });
    }
    Ok(InstallRecord { id: entry.id.clone(), version: entry.version.clone(), bins })
}

/// The installed version dir (preferring the manifest's version) and its record.
pub fn installed(entry: &EngineEntry, root: &Path) -> Option<(PathBuf, InstallRecord)> {
    let engine_dir = root.join(&entry.id);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&engine_dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort_by_key(|p| p.file_name() != Some(std::ffi::OsStr::new(&entry.version)));
    dirs.into_iter().find_map(|d| {
        let rec: InstallRecord = serde_json::from_slice(&fs::read(d.join(RECORD)).ok()?).ok()?;
        Some((d, rec))
    })
}

pub fn read_status(entry: &EngineEntry, root: &Path) -> EngineStatus {
    match installed(entry, root) {
        None if root.join(&entry.id).exists() => EngineStatus::Damaged,
        None => EngineStatus::NotInstalled,
        Some((dir, rec)) => {
            if dir.join(DAMAGED).exists() || rec.bins.values().any(|b| !dir.join(&b.rel).is_file()) {
                EngineStatus::Damaged
            } else {
                EngineStatus::Installed { update_available: rec.version != entry.version, version: rec.version }
            }
        }
    }
}

/// Re-hash every bin; on mismatch mark the install Damaged.
pub fn verify(entry: &EngineEntry, root: &Path) -> Result<bool, String> {
    let (dir, rec) = installed(entry, root).ok_or("not installed")?;
    let ok = rec.bins.values().all(|b| sha256_file(&dir.join(&b.rel)).map(|h| h == b.sha256).unwrap_or(false));
    if ok {
        let _ = fs::remove_file(dir.join(DAMAGED));
    } else {
        fs::write(dir.join(DAMAGED), b"").map_err(|e| e.to_string())?;
    }
    Ok(ok)
}

pub fn remove(entry: &EngineEntry, root: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(root.join(&entry.id)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}
```

- [ ] **Step 5: Run the install tests**

Run: `cargo test engines::install`
Expected: 4 passed.

- [ ] **Step 6: Write the `Engines` manager** — replace `engines/mod.rs`

```rust
//! Engines: download on demand, verify, locate (spec §6.3).
pub mod caps;
pub mod install;
pub mod manifest;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use install::{EngineStatus, Fetcher, InstallCtx};
use manifest::{EngineEntry, Manifest};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineEvent {
    Progress { id: String, pct: f32 },
    Done { id: String },
    Failed { id: String, error: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct EngineRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub licence: String,
    pub homepage: String,
    pub source_url: Option<String>,
    pub status: EngineStatus,
    pub disk_bytes: u64,
}

type Listener = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct Engines {
    manifest: Manifest,
    root: PathBuf,
    sevenzip: PathBuf,
    fetcher: Box<dyn Fetcher>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    installing: Mutex<HashMap<String, f32>>,
    listener: RwLock<Option<Listener>>,
}

impl Engines {
    pub fn new(manifest: Manifest, root: PathBuf, sevenzip: PathBuf, fetcher: Box<dyn Fetcher>) -> Self {
        Self {
            manifest,
            root,
            sevenzip,
            fetcher,
            locks: Mutex::new(HashMap::new()),
            installing: Mutex::new(HashMap::new()),
            listener: RwLock::new(None),
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_listener(&self, f: Listener) {
        *self.listener.write().unwrap() = Some(f);
    }

    fn emit(&self, e: EngineEvent) {
        if let Some(l) = self.listener.read().unwrap().as_ref() {
            l(e);
        }
    }

    fn entry(&self, id: &str) -> Result<&EngineEntry, String> {
        self.manifest.get(id).ok_or_else(|| format!("unknown engine '{id}'"))
    }

    pub fn status(&self, id: &str) -> EngineStatus {
        if let Some(p) = self.installing.lock().unwrap().get(id) {
            return EngineStatus::Installing { pct: *p };
        }
        match self.entry(id) {
            Ok(e) => install::read_status(e, &self.root),
            Err(_) => EngineStatus::NotInstalled,
        }
    }

    pub fn is_ready(&self, id: &str) -> bool {
        matches!(self.status(id), EngineStatus::Installed { .. })
    }

    pub fn list(&self) -> Vec<EngineRow> {
        self.manifest
            .engines
            .iter()
            .map(|e| EngineRow {
                id: e.id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
                size: e.size,
                licence: e.licence.clone(),
                homepage: e.homepage.clone(),
                source_url: e.source_url.clone(),
                status: self.status(&e.id),
                disk_bytes: crate::fsutil::dir_size(&self.root.join(&e.id)),
            })
            .collect()
    }

    /// Install if not usable yet (Installed with an update pending still counts as usable).
    pub fn ensure(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        self.install_inner(id, false, progress)
    }

    /// Library → Download / Update / Repair: installs unless the manifest version is
    /// already installed and healthy.
    pub fn install(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        self.install_inner(id, true, progress)
    }

    fn install_inner(&self, id: &str, force: bool, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let lock = self.locks.lock().unwrap().entry(id.to_string()).or_default().clone();
        let _held = lock.lock().unwrap();
        let current = install::read_status(&entry, &self.root);
        // ensure(): anything usable is enough. install() (Library): skip only when the
        // manifest version is already installed and healthy, e.g. Download clicked twice.
        let skip = match current {
            EngineStatus::Installed { update_available, .. } => !force || !update_available,
            _ => false,
        };
        if skip {
            return Ok(());
        }
        self.installing.lock().unwrap().insert(id.to_string(), 0.0);
        let ctx = InstallCtx { root: &self.root, sevenzip: &self.sevenzip, fetcher: &*self.fetcher };
        let mut last = Instant::now() - Duration::from_secs(1);
        let res = install::install(&entry, &ctx, &mut |p| {
            self.installing.lock().unwrap().insert(id.to_string(), p);
            progress(p);
            if last.elapsed() >= Duration::from_millis(150) || p >= 1.0 {
                last = Instant::now();
                self.emit(EngineEvent::Progress { id: id.to_string(), pct: p });
            }
        })
        .and_then(|_| self.write_caps(&entry));
        self.installing.lock().unwrap().remove(id);
        match &res {
            Ok(()) => self.emit(EngineEvent::Done { id: id.to_string() }),
            Err(e) => {
                log::warn!("engine {id} install failed: {e}");
                self.emit(EngineEvent::Failed { id: id.to_string(), error: e.clone() });
            }
        }
        res
    }

    fn write_caps(&self, entry: &EngineEntry) -> Result<(), String> {
        if entry.capabilities.as_deref() != Some("magick_formats") {
            return Ok(());
        }
        let (dir, _) = install::installed(entry, &self.root).ok_or("ImageMagick vanished after install")?;
        let magick = self.bin(&entry.id, "magick").ok_or("magick.exe missing")?;
        let out = crate::proc::run_capture(&magick, ["-list", "format"], Duration::from_secs(30))
            .ok_or("`magick -list format` failed")?;
        let caps = caps::parse_magick_formats(&out);
        std::fs::write(dir.join(install::CAPS), serde_json::to_vec(&caps).unwrap()).map_err(|e| e.to_string())
    }

    /// ImageMagick's recorded format capabilities, if installed.
    pub fn magick_caps(&self) -> Option<BTreeSet<String>> {
        let (dir, _) = install::installed(self.entry("imagemagick").ok()?, &self.root)?;
        serde_json::from_slice(&std::fs::read(dir.join(install::CAPS)).ok()?).ok()
    }

    pub fn bin(&self, id: &str, bin: &str) -> Option<PathBuf> {
        let (dir, rec) = install::installed(self.entry(id).ok()?, &self.root)?;
        rec.bins.get(bin).map(|b| dir.join(&b.rel))
    }

    pub fn verify(&self, id: &str) -> Result<bool, String> {
        install::verify(self.entry(id)?, &self.root)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let lock = self.locks.lock().unwrap().entry(id.to_string()).or_default().clone();
        let _held = lock.lock().unwrap();
        install::remove(&entry, &self.root).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::install::tests::{entry, make_archive, sevenzip};
    use super::install::LocalFetcher;
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(LocalFetcher, Arc<AtomicUsize>);
    impl Fetcher for Counting {
        fn fetch(&self, url: &str, dest: &Path, p: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
            self.1.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            self.0.fetch(url, dest, p)
        }
    }

    #[test]
    fn concurrent_ensure_downloads_once_and_reports_events() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let manifest = Manifest { engines: vec![entry(&zip, "1.0")] };
        let count = Arc::new(AtomicUsize::new(0));
        let engines = Arc::new(Engines::new(manifest, d.path().join("engines"), sevenzip(), Box::new(Counting(LocalFetcher { source: zip }, count.clone()))));
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev2 = events.clone();
        engines.set_listener(Arc::new(move |e| ev2.lock().unwrap().push(e)));
        let a = { let e = engines.clone(); std::thread::spawn(move || e.ensure("tool", &mut |_| {})) };
        let b = { let e = engines.clone(); std::thread::spawn(move || e.ensure("tool", &mut |_| {})) };
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(engines.is_ready("tool"));
        assert!(engines.bin("tool", "tool").unwrap().is_file());
        assert!(events.lock().unwrap().contains(&EngineEvent::Done { id: "tool".into() }));
        assert_eq!(engines.list()[0].status, EngineStatus::Installed { version: "1.0".into(), update_available: false });
    }
}
```

Make the install test helpers reachable: in `install.rs` change `#[cfg(test)] mod tests` to `#[cfg(test)] pub(crate) mod tests`.

- [ ] **Step 7: Run all engine tests**

Run: `cargo test engines::`
Expected: all pass (manifest 3, caps 1, install 4, manager 1).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(engines): install with checksum, 7z unpack, verify, capabilities and manager"
```

---

### Task 11: Job helpers — progress, explanations, maths

**Files:**
- Create: `src-tauri/src/jobs/mod.rs`, `src-tauri/src/jobs/progress.rs`, `src-tauri/src/jobs/explain.rs`, `src-tauri/src/jobs/math.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod jobs;`)

- [ ] **Step 1: Create `jobs/mod.rs`** (grows in Task 14)

```rust
//! The job queue and lifecycle (spec §6.4, §7).
pub mod explain;
pub mod math;
pub mod progress;
```

- [ ] **Step 2: Write `jobs/progress.rs` test-first**

Tests (bottom of the file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_progress_lines() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, Some(10.0));
        assert_eq!(t.feed("frame=12"), Feed { consumed: true, advanced: false, pct: None });
        assert_eq!(t.feed("out_time_us=5000000"), Feed { consumed: true, advanced: true, pct: Some(0.5) });
        assert_eq!(t.feed("out_time_us=5000000"), Feed { consumed: true, advanced: false, pct: None }, "no movement is not progress");
        assert_eq!(t.feed("out_time_us=N/A"), Feed { consumed: true, advanced: false, pct: None });
        assert_eq!(t.feed("progress=end"), Feed { consumed: true, advanced: true, pct: Some(1.0) });
        assert_eq!(t.feed("[mp3 @ 0x1] Some warning"), Feed::default());
    }

    #[test]
    fn ffmpeg_without_duration_still_advances() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, None);
        assert_eq!(t.feed("out_time_us=100"), Feed { consumed: true, advanced: true, pct: None });
    }

    #[test]
    fn ghostscript_pages() {
        let mut t = Tracker::new(ProgressKind::GhostscriptPages, None);
        assert_eq!(t.feed("GPL Ghostscript 10.04.0 (2024-09-18)"), Feed::default());
        assert_eq!(t.feed("Processing pages 1 through 4."), Feed { consumed: true, advanced: true, pct: Some(0.0) });
        assert_eq!(t.feed("Page 1"), Feed { consumed: true, advanced: true, pct: Some(0.0) });
        assert_eq!(t.feed("Page 3"), Feed { consumed: true, advanced: true, pct: Some(0.5) });
    }

    #[test]
    fn stall_timer_arms_only_between_first_output_and_the_end() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, Some(10.0));
        assert!(!t.stall_armed());
        t.feed("out_time_us=N/A");
        assert!(!t.stall_armed(), "GIF palettegen: no output yet");
        t.feed("out_time_us=0");
        assert!(!t.stall_armed());
        t.feed("out_time_us=2000000");
        assert!(t.stall_armed());
        t.feed("out_time_us=9950000");
        assert!(!t.stall_armed(), "finishing the file (+faststart)");
    }

    #[test]
    fn none_consumes_nothing() {
        let mut t = Tracker::new(ProgressKind::None, None);
        assert_eq!(t.feed("out_time_us=5"), Feed::default());
    }
}
```

Implementation (above the tests):
```rust
//! Progress from engine stdout (spec §6.4).
use crate::presets::ProgressKind;

#[derive(Debug, Default, PartialEq)]
pub struct Feed {
    /// A progress line (not worth keeping in the stderr tail).
    pub consumed: bool,
    /// Real forward movement: resets the stall watchdog.
    pub advanced: bool,
    /// 0..=1 when known.
    pub pct: Option<f32>,
}

pub struct Tracker {
    kind: ProgressKind,
    duration_s: Option<f64>,
    last_us: i64,
    total_pages: Option<u32>,
    last_page: u32,
}

impl Tracker {
    pub fn new(kind: ProgressKind, duration_s: Option<f64>) -> Self {
        Self { kind, duration_s, last_us: -1, total_pages: None, last_page: 0 }
    }

    pub fn feed(&mut self, line: &str) -> Feed {
        let line = line.trim();
        match self.kind {
            ProgressKind::Ffmpeg => self.ffmpeg(line),
            ProgressKind::GhostscriptPages => self.gs(line),
            ProgressKind::None => Feed::default(),
        }
    }

    /// Whether the stall timer applies yet. ffmpeg is silent until the first output
    /// timestamp (palettegen for GIF decodes the whole input first) and again while
    /// finishing (+faststart rewrites the file); in those windows only the hard cap applies.
    pub fn stall_armed(&self) -> bool {
        match self.kind {
            ProgressKind::Ffmpeg => {
                let near_end = self.duration_s.is_some_and(|d| d > 0.0 && self.last_us as f64 / 1e6 >= d * 0.99);
                self.last_us > 0 && !near_end
            }
            ProgressKind::GhostscriptPages => self.total_pages.is_some() || self.last_page > 0,
            ProgressKind::None => false,
        }
    }

    fn ffmpeg(&mut self, line: &str) -> Feed {
        let Some((k, v)) = line.split_once('=') else { return Feed::default() };
        if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            return Feed::default();
        }
        let mut f = Feed { consumed: true, ..Default::default() };
        if k == "out_time_us" {
            if let Ok(us) = v.trim().parse::<i64>() {
                if us > self.last_us {
                    self.last_us = us;
                    f.advanced = true;
                    if let Some(d) = self.duration_s.filter(|d| *d > 0.0) {
                        f.pct = Some(((us as f64 / 1e6) / d).clamp(0.0, 1.0) as f32);
                    }
                }
            }
        } else if k == "progress" && v.trim() == "end" {
            f.advanced = true;
            f.pct = Some(1.0);
        }
        f
    }

    fn gs(&mut self, line: &str) -> Feed {
        if let Some(rest) = line.strip_prefix("Processing pages ") {
            self.total_pages = rest.trim_end_matches('.').rsplit(' ').next().and_then(|t| t.parse().ok());
            return Feed { consumed: true, advanced: true, pct: Some(0.0) };
        }
        if let Some(n) = line.strip_prefix("Page ").and_then(|t| t.trim().parse::<u32>().ok()) {
            self.last_page = self.last_page.max(n);
            let pct = self.total_pages.map(|t| ((n.saturating_sub(1)) as f32 / t.max(1) as f32).min(1.0));
            return Feed { consumed: true, advanced: true, pct };
        }
        Feed::default()
    }
}
```

Run: `cargo test jobs::progress`
Expected: 5 passed.

- [ ] **Step 3: Write `jobs/explain.rs` test-first**

```rust
//! Plain-language reasons from engine output (spec §7, ENGINE_FAILED).
const RULES: &[(&[&str], &str)] = &[
    (&["no space left on device", "not enough space on the disk"], "The disk is full."),
    (&["password", "encrypted"], "The PDF is password-protected."),
    (&["invalid data found when processing input", "moov atom not found", "corrupt", "damaged", "error reading"], "The file looks damaged or incomplete."),
    (&["unknown encoder", "encoder not found"], "This build of the converter can't write that format."),
    (&["does not contain any stream", "matches no streams", "output file is empty"], "The file has no audio or video the converter could use."),
    (&["no decode delegate", "nodecodedelegateforthisimageformat", "no encode delegate"], "The installed ImageMagick can't handle this image format."),
    (&["unknown input format", "unknown reader", "unknown output format"], "The converter doesn't recognise this document format."),
    (&["permission denied", "access is denied"], "Windows refused access to the file."),
];

pub const FALLBACK: &str = "The converter reported an error.";

pub fn explain(lines: &[String]) -> String {
    let lower: Vec<String> = lines.iter().map(|l| l.to_lowercase()).collect();
    for (needles, msg) in RULES {
        if needles.iter().any(|n| lower.iter().any(|l| l.contains(n))) {
            return msg.to_string();
        }
    }
    FALLBACK.to_string()
}

pub fn is_disk_full(lines: &[String]) -> bool {
    explain(lines) == RULES[0].1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn maps_known_patterns() {
        assert_eq!(explain(&v(&["x.mov: Invalid data found when processing input"])), "The file looks damaged or incomplete.");
        assert_eq!(explain(&v(&["**** Error: This file requires a password for access."])), "The PDF is password-protected.");
        assert_eq!(explain(&v(&["magick: no decode delegate for this image format `HEIC'"])), "The installed ImageMagick can't handle this image format.");
        assert!(is_disk_full(&v(&["av_interleaved_write_frame(): No space left on device"])));
        assert_eq!(explain(&v(&["something odd"])), FALLBACK);
    }
}
```

Run: `cargo test jobs::explain`
Expected: 1 passed.

- [ ] **Step 4: Write `jobs/math.rs` test-first**

```rust
//! Shrink bitrate and disk-space estimates (spec §5.1 Shrink, §7 DISK_FULL).
use crate::presets::Preset;

pub const MB: u64 = 1024 * 1024;

/// Video kbps so a two-pass encode lands ≤ target (3% container margin).
/// Err(min_mb) when the result would fall below 150 kbps.
pub fn shrink_video_kbps(target_mb: u32, duration_s: f64, has_audio: bool) -> Result<u32, f64> {
    let target_bits = target_mb as f64 * MB as f64 * 8.0;
    let audio = if has_audio { 128.0 } else { 0.0 };
    let kbps = target_bits * 0.97 / duration_s / 1000.0 - audio;
    if kbps < 150.0 {
        Err((150.0 + audio) * 1000.0 * duration_s / 0.97 / 8.0 / MB as f64)
    } else {
        Ok(kbps.floor() as u32)
    }
}

/// Rough output size (spec §7): Shrink = target; WAV/AIFF ≈ 176 kB/s;
/// PDF → images ≈ 1 MB/page; otherwise ≈ the input size.
pub fn estimate_bytes(preset: &Preset, inputs_size: u64, duration_s: Option<f64>, pages: Option<u32>, target_mb: u32) -> u64 {
    if preset.uses("video_kbps") {
        return target_mb as u64 * MB;
    }
    if matches!(preset.out_ext.as_str(), "wav" | "aiff") {
        if let Some(d) = duration_s {
            return (d * 176_400.0) as u64;
        }
    }
    if preset.engine == "ghostscript" && preset.out_naming.is_folder() {
        return pages.map(|p| p as u64 * MB).unwrap_or(inputs_size.saturating_mul(10));
    }
    inputs_size
}

pub fn required_free(estimate: u64) -> u64 {
    ((estimate as f64 * 1.2) as u64).max(50 * MB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{Catalog, BUNDLED};

    #[test]
    fn shrink_bitrate() {
        assert_eq!(shrink_video_kbps(25, 100.0, true), Ok(1906));
        assert_eq!(shrink_video_kbps(25, 100.0, false), Ok(2034));
        let min = shrink_video_kbps(25, 3600.0, true).unwrap_err();
        assert!((min - 123.0).abs() < 1.0, "{min}");
    }

    #[test]
    fn estimates() {
        let c = Catalog::parse(BUNDLED).unwrap();
        assert_eq!(estimate_bytes(c.get("video-shrink").unwrap(), 900 * MB, Some(60.0), None, 25), 25 * MB);
        assert_eq!(estimate_bytes(c.get("video-wav").unwrap(), MB, Some(10.0), None, 25), 1_764_000);
        assert_eq!(estimate_bytes(c.get("pdf-images").unwrap(), MB, None, Some(30), 25), 30 * MB);
        assert_eq!(estimate_bytes(c.get("video-mp4").unwrap(), 7 * MB, None, None, 25), 7 * MB);
        assert_eq!(required_free(MB), 50 * MB);
        assert_eq!(required_f(100), 120);
    }

    fn required_f(mb: u64) -> u64 {
        required_free(mb * MB) / MB
    }
}
```

Run: `cargo test jobs::math`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(jobs): progress trackers, stderr explanations, shrink and disk maths"
```

---

### Task 12: Process runner, fake engine and watchdog

**Files:**
- Create: `src-tauri/examples/fake_engine.rs`, `src-tauri/src/testutil.rs`, `src-tauri/src/jobs/runner.rs`
- Modify: `src-tauri/src/jobs/mod.rs` (add `pub mod runner;`), `src-tauri/src/lib.rs` (add `#[cfg(test)] mod testutil;`)

- [ ] **Step 1: Write the fake engine** — `src-tauri/examples/fake_engine.rs`

```rust
//! Test double for engine processes. `cargo test` builds it into target/<profile>/examples/.
//! Usage: fake_engine <mode> [args…]
use std::io::Write;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

fn hang() -> ! {
    loop {
        sleep(Duration::from_secs(1));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::io::stdout();
    match args.get(1).map(String::as_str).unwrap_or("ok") {
        // ffmpeg-style progress, then write "converted" to args[2]
        "ok" => {
            for i in 0..5 {
                let mut o = out.lock();
                writeln!(o, "out_time_us={}", i * 200_000).unwrap();
                writeln!(o, "progress=continue").unwrap();
                o.flush().unwrap();
                sleep(Duration::from_millis(40));
            }
            println!("progress=end");
            if let Some(p) = args.get(2) {
                std::fs::write(p, b"converted").unwrap();
            }
        }
        "fail" => {
            eprintln!("Invalid data found when processing input");
            std::process::exit(3);
        }
        "hang_after_progress" => {
            println!("out_time_us=100000");
            out.lock().flush().unwrap();
            hang();
        }
        "hang" => hang(),
        // starts a grandchild that hangs, reports its pid, then hangs itself
        "spawn_child" => {
            let child = Command::new(std::env::current_exe().unwrap()).arg("hang").spawn().unwrap();
            println!("child={}", child.id());
            out.lock().flush().unwrap();
            hang();
        }
        // 20k stderr lines: a runner that doesn't drain stderr deadlocks here
        "chatty" => {
            for i in 0..20_000 {
                eprintln!("stderr line {i} ................................................................");
            }
            println!("progress=end");
        }
        // write N bytes to args[2]
        "write_out" => std::fs::write(&args[2], vec![0u8; args[3].parse().unwrap()]).unwrap(),
        // write two pages into the folder args[2]
        "write_dir" => {
            std::fs::write(std::path::Path::new(&args[2]).join("page-1.txt"), b"1").unwrap();
            std::fs::write(std::path::Path::new(&args[2]).join("page-2.txt"), b"2").unwrap();
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(64);
        }
    }
}
```

- [ ] **Step 2: Write `testutil.rs`**

```rust
//! Test-only helpers.
use std::path::PathBuf;

/// target/<profile>/examples/fake_engine.exe (built by plain `cargo test`).
pub fn fake_engine() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    let exe = p.join("examples").join("fake_engine.exe");
    assert!(exe.exists(), "{} is missing: run plain `cargo test` (it builds examples)", exe.display());
    exe
}
```

- [ ] **Step 3: Write the failing runner tests** — bottom of `jobs/runner.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fake_engine;

    fn spec(dir: &std::path::Path, args: &[&str], progress: ProgressKind, stall_ms: u64) -> RunSpec {
        RunSpec {
            program: fake_engine(),
            args: args.iter().map(OsString::from).collect(),
            cwd: dir.to_path_buf(),
            progress,
            duration_s: Some(1.0),
            stall: Duration::from_millis(stall_ms),
            hard_cap: Duration::from_millis(stall_ms),
        }
    }

    #[test]
    fn success_reports_progress_and_writes_output() {
        let d = tempfile::tempdir().unwrap();
        let mut pcts = Vec::new();
        let r = run(&spec(d.path(), &["ok", "out.txt"], ProgressKind::Ffmpeg, 5000), &CancelToken::default(), &mut |p| pcts.push(p)).unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(0));
        assert_eq!(pcts.last(), Some(&1.0));
        assert!(pcts.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(std::fs::read_to_string(d.path().join("out.txt")).unwrap(), "converted");
        assert!(r.tail.is_empty(), "progress lines are not kept: {:?}", r.tail);
    }

    #[test]
    fn failure_keeps_stderr_tail() {
        let d = tempfile::tempdir().unwrap();
        let r = run(&spec(d.path(), &["fail"], ProgressKind::None, 5000), &CancelToken::default(), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(3));
        assert!(r.tail.iter().any(|l| l.contains("Invalid data")));
    }

    #[test]
    fn stall_watchdog_kills_silent_engine() {
        let d = tempfile::tempdir().unwrap();
        let t = Instant::now();
        let r = run(&spec(d.path(), &["hang_after_progress"], ProgressKind::Ffmpeg, 500), &CancelToken::default(), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Stalled);
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn hard_cap_applies_when_engine_reports_no_progress() {
        let d = tempfile::tempdir().unwrap();
        let r = run(&spec(d.path(), &["hang"], ProgressKind::None, 500), &CancelToken::default(), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Stalled);
    }

    #[test]
    fn cancel_kills_the_whole_tree() {
        let d = tempfile::tempdir().unwrap();
        let cancel = CancelToken::default();
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            c2.cancel();
        });
        let r = run(&spec(d.path(), &["spawn_child"], ProgressKind::None, 60_000), &cancel, &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Cancelled);
        let pid: u32 = r.tail.iter().find_map(|l| l.strip_prefix("child=")).expect("child pid in tail").parse().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!crate::platform::is_process_alive(pid), "grandchild survived");
    }

    #[test]
    fn chatty_stderr_does_not_deadlock() {
        let d = tempfile::tempdir().unwrap();
        let t = Instant::now();
        let r = run(&spec(d.path(), &["chatty"], ProgressKind::None, 20_000), &CancelToken::default(), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(0));
        assert_eq!(r.tail.len(), TAIL_LINES);
        assert!(t.elapsed() < Duration::from_secs(15));
    }
}
```

Run: `cargo test jobs::runner`
Expected: FAIL to compile.

- [ ] **Step 4: Implement `jobs/runner.rs`** (above the tests)

```rust
//! Run one engine process (spec §6.4): Job Object, both pipes drained, watchdog.
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::progress::Tracker;
use crate::platform;
use crate::presets::ProgressKind;

pub const TAIL_LINES: usize = 30;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct RunSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub progress: ProgressKind,
    pub duration_s: Option<f64>,
    /// No forward progress for this long → Stalled (engines that report progress).
    pub stall: Duration,
    /// Total runtime cap for engines that report no progress.
    pub hard_cap: Duration,
}

#[derive(Debug, PartialEq)]
pub enum RunOutcome {
    Exited(i32),
    Stalled,
    Cancelled,
}

pub struct RunResult {
    pub outcome: RunOutcome,
    /// Last TAIL_LINES non-progress lines from stdout and stderr.
    pub tail: Vec<String>,
}

enum Line {
    Out(String),
    Err(String),
}

fn spawn_reader<R: Read + Send + 'static>(r: R, tx: Sender<Line>, wrap: fn(String) -> Line) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let s = String::from_utf8_lossy(&buf);
                    for part in s.split('\r') {
                        let t = part.trim_end_matches('\n');
                        if !t.trim().is_empty() && tx.send(wrap(t.to_string())).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    })
}

fn absorb(line: Line, tracker: &mut Tracker, tail: &mut VecDeque<String>, last_advance: &mut Instant, on_pct: &mut dyn FnMut(f32)) {
    let text = match line {
        Line::Out(s) => {
            let f = tracker.feed(&s);
            if f.advanced {
                *last_advance = Instant::now();
            }
            if let Some(p) = f.pct {
                on_pct(p);
            }
            if f.consumed {
                return;
            }
            s
        }
        Line::Err(s) => s,
    };
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(text);
}

pub fn run(spec: &RunSpec, cancel: &CancelToken, on_pct: &mut dyn FnMut(f32)) -> io::Result<RunResult> {
    let group = platform::KillGroup::new()?;
    let mut child = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(platform::CREATE_NO_WINDOW)
        .spawn()?;
    if let Err(e) = group.assign(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let (tx, rx) = mpsc::channel();
    let readers = [
        spawn_reader(child.stdout.take().expect("piped stdout"), tx.clone(), Line::Out),
        spawn_reader(child.stderr.take().expect("piped stderr"), tx, Line::Err),
    ];
    let mut tracker = Tracker::new(spec.progress, spec.duration_s);
    let mut tail = VecDeque::new();
    let start = Instant::now();
    let mut last_advance = start;

    let outcome = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => absorb(line, &mut tracker, &mut tail, &mut last_advance, on_pct),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(20)),
        }
        if cancel.is_cancelled() {
            group.kill();
            break RunOutcome::Cancelled;
        }
        let stalled = match spec.progress {
            ProgressKind::None => start.elapsed() > spec.hard_cap,
            _ if tracker.stall_armed() => last_advance.elapsed() > spec.stall,
            // before the first output / while finishing: only the hard cap applies
            _ => last_advance.elapsed() > spec.hard_cap,
        };
        if stalled {
            group.kill();
            break RunOutcome::Stalled;
        }
        if let Some(status) = child.try_wait()? {
            break RunOutcome::Exited(status.code().unwrap_or(-1));
        }
    };
    let _ = child.wait();
    group.kill(); // anything the engine left behind would hold our pipes open
    for r in readers {
        let _ = r.join();
    }
    for line in rx.try_iter() {
        absorb(line, &mut tracker, &mut tail, &mut last_advance, on_pct);
    }
    Ok(RunResult { outcome, tail: tail.into() })
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test jobs::runner`
Expected: 6 passed. If `cancel_kills_the_whole_tree` fails because the pid line isn't in the tail, check that `spawn_child` flushes stdout before hanging.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(jobs): process runner with job objects, pipe draining and stall watchdog"
```

---

### Task 13: `history` (SQLite)

**Files:**
- Create: `src-tauri/src/history.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod history;`)

- [ ] **Step 1: Write the failing tests** — bottom of `history.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> History {
        History::open_in_memory().unwrap()
    }

    #[test]
    fn insert_finish_get_list() {
        let h = h();
        h.insert("a", "video-mp3", &["C:\\x\\a.mov".into()], 100).unwrap();
        h.insert("b", "video-mp4", &["C:\\x\\b.mov".into()], 101).unwrap();
        h.finish("a", &Finish { status: "done", output: Some("C:\\x\\a.mp3"), ..Default::default() }, 110).unwrap();
        let a = h.get("a").unwrap().unwrap();
        assert_eq!(a.status, "done");
        assert_eq!(a.output.as_deref(), Some("C:\\x\\a.mp3"));
        assert_eq!(a.inputs, vec!["C:\\x\\a.mov".to_string()]);
        assert_eq!(a.ended_at, Some(110));
        let list = h.list(0, 50).unwrap();
        assert_eq!(list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["b", "a"], "newest first");
    }

    #[test]
    fn recovery_marks_unfinished_jobs_interrupted_and_returns_temp_dirs() {
        let h = h();
        h.insert("a", "p", &[], 1).unwrap();
        h.set_status("a", "running").unwrap();
        h.set_temp("a", Some("C:\\x\\.recast-a")).unwrap();
        h.insert("b", "p", &[], 2).unwrap();
        h.finish("b", &Finish { status: "done", ..Default::default() }, 3).unwrap();
        assert_eq!(h.recover_interrupted(9).unwrap(), vec!["C:\\x\\.recast-a".to_string()]);
        let a = h.get("a").unwrap().unwrap();
        assert_eq!(a.status, "interrupted");
        assert_eq!(a.error_code.as_deref(), Some("INTERRUPTED"));
        assert_eq!(a.temp_path, None);
        assert_eq!(h.get("b").unwrap().unwrap().status, "done");
    }

    #[test]
    fn prune_keeps_newest_finished_and_all_unfinished() {
        let h = h();
        for i in 0..10 {
            let id = format!("j{i}");
            h.insert(&id, "p", &[], i).unwrap();
            if i != 0 {
                h.finish(&id, &Finish { status: "done", ..Default::default() }, i).unwrap();
            }
        }
        h.prune(3).unwrap();
        let ids: Vec<String> = h.list(0, 50).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["j9", "j8", "j7", "j0"]);
    }

    #[test]
    fn remove_deletes_only_the_row() {
        let h = h();
        h.insert("a", "p", &[], 1).unwrap();
        h.remove("a").unwrap();
        assert!(h.get("a").unwrap().is_none());
    }

    #[test]
    fn reopening_a_file_keeps_rows() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("recast.db");
        History::open(&p).unwrap().insert("a", "p", &[], 1).unwrap();
        assert!(History::open(&p).unwrap().get("a").unwrap().is_some());
    }
}
```

Run: `cargo test history::`
Expected: FAIL to compile.

- [ ] **Step 2: Implement** (above the tests)

```rust
//! Job history in SQLite at %LOCALAPPDATA%\Recast\recast.db (spec §6.5).
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

const SCHEMA_VERSION: &str = "1";
const COLS: &str = "id, preset_id, inputs, output, status, error_code, error_detail, stderr_tail, started_at, ended_at, temp_path";
const UNFINISHED: &str = "('queued','waiting_engine','running')";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub preset_id: String,
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub stderr_tail: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub temp_path: Option<String>,
}

#[derive(Default)]
pub struct Finish<'a> {
    pub status: &'a str,
    pub output: Option<&'a str>,
    pub error_code: Option<&'a str>,
    pub error_detail: Option<&'a str>,
    pub stderr_tail: Option<&'a str>,
}

pub struct History {
    conn: Connection,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<JobRecord> {
    let inputs: String = r.get("inputs")?;
    Ok(JobRecord {
        id: r.get("id")?,
        preset_id: r.get("preset_id")?,
        inputs: serde_json::from_str(&inputs).unwrap_or_default(),
        output: r.get("output")?,
        status: r.get("status")?,
        error_code: r.get("error_code")?,
        error_detail: r.get("error_detail")?,
        stderr_tail: r.get("stderr_tail")?,
        started_at: r.get("started_at")?,
        ended_at: r.get("ended_at")?,
        temp_path: r.get("temp_path")?,
    })
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let h = Self { conn: Connection::open(path)? };
        h.migrate()?;
        Ok(h)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let h = Self { conn: Connection::open_in_memory()? };
        h.migrate()?;
        Ok(h)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS jobs (
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               id TEXT NOT NULL UNIQUE,
               preset_id TEXT NOT NULL,
               inputs TEXT NOT NULL,
               output TEXT,
               status TEXT NOT NULL,
               error_code TEXT,
               error_detail TEXT,
               stderr_tail TEXT,
               started_at INTEGER NOT NULL,
               ended_at INTEGER,
               temp_path TEXT);",
        )?;
        self.conn.execute("INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)", params![SCHEMA_VERSION])?;
        Ok(())
    }

    pub fn insert(&self, id: &str, preset_id: &str, inputs: &[String], now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO jobs (id, preset_id, inputs, status, started_at) VALUES (?1, ?2, ?3, 'queued', ?4)",
            params![id, preset_id, serde_json::to_string(inputs).unwrap(), now],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE jobs SET status = ?2 WHERE id = ?1", params![id, status])?;
        Ok(())
    }

    pub fn set_temp(&self, id: &str, temp: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE jobs SET temp_path = ?2 WHERE id = ?1", params![id, temp])?;
        Ok(())
    }

    pub fn finish(&self, id: &str, f: &Finish, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = ?2, output = ?3, error_code = ?4, error_detail = ?5, stderr_tail = ?6,
                             ended_at = ?7, temp_path = NULL WHERE id = ?1",
            params![id, f.status, f.output, f.error_code, f.error_detail, f.stderr_tail, now],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<JobRecord>> {
        self.conn.query_row(&format!("SELECT {COLS} FROM jobs WHERE id = ?1"), params![id], row).optional()
    }

    pub fn list(&self, page: u32, per_page: u32) -> rusqlite::Result<Vec<JobRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM jobs ORDER BY seq DESC LIMIT ?1 OFFSET ?2"))?;
        let rows = st.query_map(params![per_page, page * per_page], row)?;
        rows.collect()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Keep the newest `keep` rows; never delete unfinished jobs.
    pub fn prune(&self, keep: u32) -> rusqlite::Result<()> {
        self.conn.execute(
            &format!(
                "DELETE FROM jobs WHERE status NOT IN {UNFINISHED}
                 AND seq NOT IN (SELECT seq FROM jobs ORDER BY seq DESC LIMIT ?1)"
            ),
            params![keep],
        )?;
        Ok(())
    }

    /// At startup: unfinished jobs become INTERRUPTED; returns their temp dirs to delete.
    pub fn recover_interrupted(&self, now: i64) -> rusqlite::Result<Vec<String>> {
        let temps: Vec<String> = {
            let mut st = self.conn.prepare(&format!("SELECT temp_path FROM jobs WHERE status IN {UNFINISHED} AND temp_path IS NOT NULL"))?;
            let rows = st.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        self.conn.execute(
            &format!(
                "UPDATE jobs SET status = 'interrupted', error_code = 'INTERRUPTED',
                 error_detail = 'Recast closed before this finished.', ended_at = ?1, temp_path = NULL
                 WHERE status IN {UNFINISHED}"
            ),
            params![now],
        )?;
        Ok(temps)
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test history::`
Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(history): SQLite job history with pruning and crash recovery"
```

---
### Task 14: `JobManager` — queue, execute, finalize

**Files:**
- Modify: `src-tauri/src/jobs/mod.rs`

- [ ] **Step 1: Write the failing tests** — append to `jobs/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fake_engine;
    use std::sync::Condvar;

    const CATALOG: &str = r#"
[[preset]]
id = "ok"
label = "OK"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]

[[preset]]
id = "fail"
label = "Fail"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['fail']]

[[preset]]
id = "hang"
label = "Hang"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['hang_after_progress']]

[[preset]]
id = "slow"
label = "Slow"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['hang']]

[[preset]]
id = "pages"
label = "Pages"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "folder:pages"
passes = [['write_dir', '{out_dir}']]

[[preset]]
id = "merge"
label = "Merge"
description = "d"
icon = "i"
kinds = ["unknown"]
min_files = 2
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "fixed:merged"
passes = [['ok', '{out}', '{inputs}']]

[[preset]]
id = "compress"
label = "Compress"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
keep_same_format = true
discard_if_not_smaller = true
out_ext = "same"
out_naming = "suffix:compressed"
passes = [['write_out', '{out}', '100000']]
"#;

    struct FakeHost {
        exe: PathBuf,
    }
    impl EngineHost for FakeHost {
        fn is_installed(&self, _: &str) -> bool {
            true
        }
        fn ensure(&self, _: &str, _: &mut dyn FnMut(f32)) -> Result<(), String> {
            Ok(())
        }
        fn bin(&self, _: &str, _: &str) -> Option<PathBuf> {
            Some(self.exe.clone())
        }
        fn probe(&self, _: &Path) -> Option<Probe> {
            Some(Probe { duration_s: Some(1.0), has_audio: true, has_video: true, audio_codec: None })
        }
        fn pdf_pages(&self, _: &Path) -> Option<u32> {
            None
        }
    }

    #[derive(Default)]
    pub(crate) struct Collect {
        evs: Mutex<Vec<JobEvent>>,
        cv: Condvar,
    }
    impl Events for Collect {
        fn emit(&self, ev: &JobEvent) {
            self.evs.lock().unwrap().push(ev.clone());
            self.cv.notify_all();
        }
    }
    const TERMINAL: &[&str] = &["done", "skipped", "failed", "cancelled"];
    impl Collect {
        fn wait_for(&self, id: &str, statuses: &[&str]) -> JobEvent {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut g = self.evs.lock().unwrap();
            loop {
                if let Some(e) = g.iter().find(|e| e.job_id == id && statuses.contains(&e.status.as_str())) {
                    return e.clone();
                }
                let left = deadline.saturating_duration_since(Instant::now());
                assert!(!left.is_zero(), "job {id} never reached {statuses:?}: {:?}", *g);
                g = self.cv.wait_timeout(g, left).unwrap().0;
            }
        }
        fn wait_end(&self, id: &str) -> JobEvent {
            self.wait_for(id, TERMINAL)
        }
        fn index_of(&self, id: &str, status: &str) -> usize {
            self.evs.lock().unwrap().iter().position(|e| e.job_id == id && e.status == status).unwrap()
        }
    }

    struct Rig {
        mgr: JobManager,
        events: Arc<Collect>,
        history: Arc<Mutex<History>>,
        dir: tempfile::TempDir,
        _scratch: tempfile::TempDir,
    }

    fn rig_with(f: impl FnOnce(&mut Settings)) -> Rig {
        let mut s = Settings::default();
        f(&mut s);
        let events = Arc::new(Collect::default());
        let history = Arc::new(Mutex::new(History::open_in_memory().unwrap()));
        let scratch = tempfile::tempdir().unwrap();
        let mgr = JobManager::new(
            Arc::new(Catalog::parse(CATALOG).unwrap()),
            Arc::new(FakeHost { exe: fake_engine() }),
            history.clone(),
            Arc::new(move || s.clone()),
            events.clone(),
            scratch.path().to_path_buf(),
        );
        Rig { mgr, events, history, dir: tempfile::tempdir().unwrap(), _scratch: scratch }
    }
    fn rig() -> Rig {
        rig_with(|_| {})
    }

    impl Rig {
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let p = self.dir.path().join(name);
            fs::write(&p, bytes).unwrap();
            p
        }
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(self.dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        }
        fn run(&self, preset: &str, files: &[PathBuf]) -> JobEvent {
            let ids = self.mgr.start_jobs(preset, files).unwrap();
            self.events.wait_end(&ids[0])
        }
    }

    #[test]
    fn converts_next_to_the_source_and_cleans_up() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        let ev = r.run("ok", &[src]);
        assert_eq!(ev.status, "done", "{:?}", ev.error);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert_eq!(out.file_name().unwrap(), "a.txt");
        assert_eq!(fs::read_to_string(&out).unwrap(), "converted");
        assert_eq!(r.names(), ["a.bin", "a.txt"], "no .recast-* temp dir left");
        assert_eq!(ev.preset_label, "OK");
        let rec = r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap();
        assert_eq!(rec.status, "done");
        assert!(rec.temp_path.is_none());
        let saw_progress = r.events.evs.lock().unwrap().iter().any(|e| e.status == "running" && e.pct.unwrap_or(0.0) > 0.0);
        assert!(saw_progress);
    }

    #[test]
    fn never_overwrites() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        r.file("a.txt", b"mine");
        let ev = r.run("ok", &[src]);
        assert!(ev.out_path.unwrap().ends_with("a (1).txt"));
        assert_eq!(fs::read_to_string(r.dir.path().join("a.txt")).unwrap(), "mine");
    }

    #[test]
    fn engine_failure_is_explained_and_leaves_nothing() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        let ev = r.run("fail", &[src]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::EngineFailed);
        assert_eq!(err.message, "The file looks damaged or incomplete.");
        assert_eq!(r.names(), ["a.bin"]);
        let rec = r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap();
        assert!(rec.stderr_tail.unwrap().contains("Invalid data"));
        assert_eq!(rec.error_code.as_deref(), Some("ENGINE_FAILED"));
    }

    #[test]
    fn stalled_engine_is_killed() {
        let r = rig_with(|s| s.jobs.stall_timeout_s = 1);
        let src = r.file("a.bin", b"source");
        let t = Instant::now();
        let ev = r.run("hang", &[src]);
        assert_eq!(ev.error.unwrap().code, ErrorCode::Stalled);
        assert!(t.elapsed() < Duration::from_secs(10));
        assert_eq!(r.names(), ["a.bin"]);
    }

    #[test]
    fn cancel_running_and_queued_jobs() {
        let r = rig_with(|s| s.jobs.concurrency = 1);
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let first = r.mgr.start_jobs("slow", &[a]).unwrap().remove(0);
        let second = r.mgr.start_jobs("ok", &[b]).unwrap().remove(0);
        r.events.wait_for(&first, &["running"]);
        r.mgr.cancel(&second);
        assert_eq!(r.events.wait_end(&second).status, "cancelled");
        r.mgr.cancel(&first);
        assert_eq!(r.events.wait_end(&first).status, "cancelled");
        assert_eq!(r.names(), ["a.bin", "b.bin"]);
    }

    #[test]
    fn concurrency_one_runs_jobs_in_turn() {
        let r = rig_with(|s| s.jobs.concurrency = 1);
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let ids = r.mgr.start_jobs("ok", &[a, b]).unwrap();
        r.events.wait_end(&ids[0]);
        r.events.wait_end(&ids[1]);
        assert!(r.events.index_of(&ids[1], "running") > r.events.index_of(&ids[0], "done"));
    }

    #[test]
    fn missing_source_fails_before_spawning() {
        let r = rig();
        let ev = r.run("ok", &[r.dir.path().join("gone.bin")]);
        assert_eq!(ev.error.unwrap().code, ErrorCode::SourceUnreadable);
    }

    #[test]
    fn multi_file_preset_is_one_job() {
        let r = rig();
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let ids = r.mgr.start_jobs("merge", &[a, b]).unwrap();
        assert_eq!(ids.len(), 1);
        let ev = r.events.wait_end(&ids[0]);
        assert!(ev.out_path.unwrap().ends_with("merged.txt"));
        assert_eq!(ev.file, "a.bin +1");
    }

    #[test]
    fn folder_output() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let ev = r.run("pages", &[src]);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert_eq!(out.file_name().unwrap(), "a (pages)");
        assert!(out.join("page-2.txt").is_file());
    }

    #[test]
    fn compress_that_isnt_smaller_is_skipped() {
        let r = rig();
        let src = r.file("a.bin", b"tiny");
        let ev = r.run("compress", &[src]);
        assert_eq!(ev.status, "skipped");
        assert_eq!(ev.error.unwrap().code, ErrorCode::AlreadySmall);
        assert_eq!(r.names(), ["a.bin"]);
    }

    #[test]
    fn retry_into_the_fallback_folder() {
        let fallback = tempfile::tempdir().unwrap();
        let fb = fallback.path().to_string_lossy().into_owned();
        let r = rig_with(move |s| s.output.fallback_folder = fb);
        let src = r.file("a.bin", b"a");
        let first = r.run("ok", &[src]);
        let again = r.mgr.retry(&first.job_id, true).unwrap().remove(0);
        let ev = r.events.wait_end(&again);
        assert!(PathBuf::from(ev.out_path.unwrap()).starts_with(fallback.path()));
    }

    #[test]
    fn long_destination_paths_use_the_scratch_temp_dir() {
        let scratch = Path::new(r"C:\Users\x\AppData\Local\Recast\tmp");
        let short = Path::new(r"C:\d");
        assert_eq!(tmp_dir_for(short, "j1", scratch).unwrap(), Path::new(r"C:\d\.recast-j1"));
        let long = PathBuf::from(format!(r"C:\{}", "x".repeat(250)));
        assert_eq!(tmp_dir_for(&long, "j1", scratch).unwrap(), scratch.join(".recast-j1"));
        let other_drive = PathBuf::from(format!(r"D:\{}", "x".repeat(250)));
        assert_eq!(tmp_dir_for(&other_drive, "j1", scratch).unwrap_err().error.code, ErrorCode::DestUnwritable);
    }
}
```

Run: `cargo test jobs::tests`
Expected: FAIL to compile.

- [ ] **Step 2: Implement** — replace the top of `jobs/mod.rs` (keep the tests module at the bottom)

```rust
//! The job queue and lifecycle (spec §6.4, §7).
pub mod explain;
pub mod math;
pub mod notify;
pub mod progress;
pub mod runner;

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::detect::{ext_of, Probe};
use crate::history::{Finish, History};
use crate::platform;
use crate::presets::command::{build_command, expand, BuildInput};
use crate::presets::{naming, pandoc_reader, Catalog, Preset, ProgressKind};
use crate::settings::Settings;
use math::MB;
pub use runner::CancelToken;
use runner::{RunOutcome, RunSpec};

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    EngineInstallFailed,
    EngineFailed,
    Stalled,
    SourceUnreadable,
    DestUnwritable,
    DestLocked,
    DiskFull,
    TargetTooSmall,
    AlreadySmall,
    Cancelled,
    Interrupted,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EngineInstallFailed => "ENGINE_INSTALL_FAILED",
            Self::EngineFailed => "ENGINE_FAILED",
            Self::Stalled => "STALLED",
            Self::SourceUnreadable => "SOURCE_UNREADABLE",
            Self::DestUnwritable => "DEST_UNWRITABLE",
            Self::DestLocked => "DEST_LOCKED",
            Self::DiskFull => "DISK_FULL",
            Self::TargetTooSmall => "TARGET_TOO_SMALL",
            Self::AlreadySmall => "ALREADY_SMALL",
            Self::Cancelled => "CANCELLED",
            Self::Interrupted => "INTERRUPTED",
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
}

/// Payload of every job://* event (spec §6.4).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobEvent {
    pub job_id: String,
    pub batch_id: String,
    pub batch_size: usize,
    pub preset_id: String,
    pub preset_label: String,
    /// Display name: first input's file name, plus " +N" for multi-file jobs.
    pub file: String,
    /// queued | waiting_engine | running | done | skipped | failed | cancelled
    pub status: String,
    pub pct: Option<f32>,
    pub eta_s: Option<f64>,
    pub out_path: Option<String>,
    pub error: Option<JobError>,
}

pub trait Events: Send + Sync {
    fn emit(&self, ev: &JobEvent);
}

/// What jobs need from engines (implemented by `Engines`; faked in tests).
pub trait EngineHost: Send + Sync {
    fn is_installed(&self, engine: &str) -> bool;
    fn ensure(&self, engine: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String>;
    fn bin(&self, engine: &str, bin: &str) -> Option<PathBuf>;
    fn probe(&self, path: &Path) -> Option<Probe>;
    fn pdf_pages(&self, path: &Path) -> Option<u32>;
}

pub type SettingsFn = Arc<dyn Fn() -> Settings + Send + Sync>;

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

#[derive(Clone)]
struct Job {
    id: String,
    batch_id: String,
    batch_size: usize,
    preset_id: String,
    inputs: Vec<PathBuf>,
    fallback: bool,
}

impl Job {
    fn display(&self) -> String {
        let first = file_name(&self.inputs[0]);
        if self.inputs.len() > 1 { format!("{first} +{}", self.inputs.len() - 1) } else { first }
    }
}

#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    running: HashMap<String, CancelToken>,
    /// job id → engine id, for jobs in `running`.
    running_engines: HashMap<String, String>,
}

struct Inner {
    catalog: Arc<Catalog>,
    host: Arc<dyn EngineHost>,
    history: Arc<Mutex<History>>,
    settings: SettingsFn,
    events: Arc<dyn Events>,
    /// %LOCALAPPDATA%\Recast\tmp: temp dirs for destinations whose path is too long.
    scratch: PathBuf,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct JobManager {
    inner: Arc<Inner>,
}

enum Finished {
    Done(PathBuf),
    Skipped(String),
}

#[derive(Debug)]
struct Failure {
    error: JobError,
    tail: Vec<String>,
}

impl Failure {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { error: JobError { code, message: message.into() }, tail: Vec::new() }
    }
}

/// Deletes the job's temp dir however the job ends (outputs are moved out first).
struct TmpGuard(PathBuf);

impl Drop for TmpGuard {
    fn drop(&mut self) {
        for _ in 0..10 {
            match fs::remove_dir_all(&self.0) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => std::thread::sleep(Duration::from_millis(100)),
                _ => return,
            }
        }
        log::warn!("couldn't remove temp dir {}", self.0.display());
    }
}

impl JobManager {
    pub fn new(
        catalog: Arc<Catalog>,
        host: Arc<dyn EngineHost>,
        history: Arc<Mutex<History>>,
        settings: SettingsFn,
        events: Arc<dyn Events>,
        scratch: PathBuf,
    ) -> Self {
        Self { inner: Arc::new(Inner { catalog, host, history, settings, events, scratch, state: Mutex::new(State::default()) }) }
    }

    /// One job per file, or one job for all files when the preset is multi-file.
    pub fn start_jobs(&self, preset_id: &str, paths: &[PathBuf]) -> Result<Vec<String>, String> {
        self.enqueue(preset_id, paths, false)
    }

    /// Start queued jobs if there's room (call after the concurrency setting changes).
    pub fn pump(&self) {
        Inner::pump(&self.inner);
    }

    /// Is a running job using this engine? (Library refuses Remove/Repair/Update then.)
    pub fn engine_in_use(&self, engine: &str) -> bool {
        let st = self.inner.state.lock().unwrap();
        st.running_engines.values().any(|e| e == engine)
    }

    pub fn retry(&self, id: &str, fallback: bool) -> Result<Vec<String>, String> {
        let rec = self.inner.history.lock().unwrap().get(id).map_err(|e| e.to_string())?.ok_or("That job is no longer in the list.")?;
        let paths: Vec<PathBuf> = rec.inputs.iter().map(PathBuf::from).collect();
        self.enqueue(&rec.preset_id, &paths, fallback)
    }

    pub fn cancel(&self, id: &str) {
        let mut st = self.inner.state.lock().unwrap();
        if let Some(pos) = st.queue.iter().position(|j| j.id == id) {
            let job = st.queue.remove(pos).expect("position is valid");
            drop(st);
            let _ = self.inner.history.lock().unwrap().finish(
                &job.id,
                &Finish { status: "cancelled", error_code: Some("CANCELLED"), ..Default::default() },
                now(),
            );
            self.inner.emit(&job, "cancelled", None, None, None, Some(JobError { code: ErrorCode::Cancelled, message: "Cancelled.".into() }));
        } else if let Some(token) = st.running.get(id) {
            token.cancel();
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        let st = self.inner.state.lock().unwrap();
        st.running.contains_key(id) || st.queue.iter().any(|j| j.id == id)
    }

    fn enqueue(&self, preset_id: &str, paths: &[PathBuf], fallback: bool) -> Result<Vec<String>, String> {
        let preset = self.inner.catalog.get(preset_id).ok_or_else(|| format!("Unknown conversion '{preset_id}'."))?;
        if paths.is_empty() {
            return Err("No files to convert.".into());
        }
        let abs: Vec<PathBuf> = paths.iter().map(std::path::absolute).collect::<io::Result<_>>().map_err(|e| e.to_string())?;
        let groups: Vec<Vec<PathBuf>> = if preset.is_multi() { vec![abs] } else { abs.into_iter().map(|p| vec![p]).collect() };
        let batch_id = uuid::Uuid::new_v4().to_string();
        let batch_size = groups.len();
        let mut ids = Vec::new();
        for inputs in groups {
            let job = Job { id: uuid::Uuid::new_v4().to_string(), batch_id: batch_id.clone(), batch_size, preset_id: preset_id.to_string(), inputs, fallback };
            let inputs_str: Vec<String> = job.inputs.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            self.inner.history.lock().unwrap().insert(&job.id, preset_id, &inputs_str, now()).map_err(|e| e.to_string())?;
            self.inner.emit(&job, "queued", None, None, None, None);
            ids.push(job.id.clone());
            self.inner.state.lock().unwrap().queue.push_back(job);
        }
        Inner::pump(&self.inner);
        Ok(ids)
    }
}

impl Inner {
    fn pump(this: &Arc<Inner>) {
        loop {
            let limit = (this.settings)().jobs.concurrency.max(1) as usize;
            let (job, token) = {
                let mut st = this.state.lock().unwrap();
                if st.running.len() >= limit {
                    return;
                }
                let Some(job) = st.queue.pop_front() else { return };
                let token = CancelToken::default();
                st.running.insert(job.id.clone(), token.clone());
                if let Some(p) = this.catalog.get(&job.preset_id) {
                    st.running_engines.insert(job.id.clone(), p.engine.clone());
                }
                (job, token)
            };
            let me = this.clone();
            std::thread::spawn(move || {
                me.execute(&job, &token);
                {
                    let mut st = me.state.lock().unwrap();
                    st.running.remove(&job.id);
                    st.running_engines.remove(&job.id);
                }
                Inner::pump(&me);
            });
        }
    }

    fn emit(&self, job: &Job, status: &str, pct: Option<f32>, eta: Option<f64>, out: Option<String>, error: Option<JobError>) {
        self.events.emit(&JobEvent {
            job_id: job.id.clone(),
            batch_id: job.batch_id.clone(),
            batch_size: job.batch_size,
            preset_id: job.preset_id.clone(),
            preset_label: self.catalog.get(&job.preset_id).map(|p| p.label.clone()).unwrap_or_default(),
            file: job.display(),
            status: status.to_string(),
            pct,
            eta_s: eta,
            out_path: out,
            error,
        });
    }

    fn execute(&self, job: &Job, cancel: &CancelToken) {
        let started = Instant::now();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_status = String::new();
        let mut progress = |status: &str, pct: Option<f32>| {
            let changed = status != last_status;
            if !changed && last_emit.elapsed() < Duration::from_millis(100) {
                return;
            }
            if changed {
                let _ = self.history.lock().unwrap().set_status(&job.id, status);
                last_status = status.to_string();
            }
            last_emit = Instant::now();
            let eta = (status == "running")
                .then_some(pct)
                .flatten()
                .filter(|p| *p > 0.02 && *p < 1.0)
                .map(|p| started.elapsed().as_secs_f64() * (1.0 - p as f64) / p as f64);
            self.emit(job, status, pct, eta, None, None);
        };
        progress("running", Some(0.0));
        let result = self.run_job(job, cancel, &mut progress);
        let h = self.history.lock().unwrap();
        match result {
            Ok(Finished::Done(out)) => {
                let o = out.to_string_lossy().into_owned();
                let _ = h.finish(&job.id, &Finish { status: "done", output: Some(&o), ..Default::default() }, now());
                drop(h);
                self.emit(job, "done", Some(1.0), None, Some(o), None);
            }
            Ok(Finished::Skipped(msg)) => {
                let _ = h.finish(&job.id, &Finish { status: "skipped", error_code: Some("ALREADY_SMALL"), error_detail: Some(&msg), ..Default::default() }, now());
                drop(h);
                self.emit(job, "skipped", None, None, None, Some(JobError { code: ErrorCode::AlreadySmall, message: msg }));
            }
            Err(f) => {
                let status = if f.error.code == ErrorCode::Cancelled { "cancelled" } else { "failed" };
                let tail = f.tail.join("\n");
                let _ = h.finish(
                    &job.id,
                    &Finish {
                        status,
                        error_code: Some(f.error.code.as_str()),
                        error_detail: Some(&f.error.message),
                        stderr_tail: (!tail.is_empty()).then_some(tail.as_str()),
                        ..Default::default()
                    },
                    now(),
                );
                drop(h);
                if status == "failed" {
                    log::warn!("job {} ({}) failed: {} {}", job.id, job.preset_id, f.error.code.as_str(), f.error.message);
                }
                self.emit(job, status, None, None, None, Some(f.error));
            }
        }
        let _ = self.history.lock().unwrap().prune(500);
    }

    fn dest_dir(&self, s: &Settings, job: &Job) -> Result<PathBuf, Failure> {
        let dir = if job.fallback {
            s.fallback_dir()
        } else if s.output.location == "folder" && !s.output.folder.trim().is_empty() {
            PathBuf::from(&s.output.folder)
        } else {
            job.inputs[0].parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let unwritable = |e: io::Error| Failure::new(ErrorCode::DestUnwritable, format!("Recast can't save files in {}. ({e})", dir.display()));
        fs::create_dir_all(&dir).map_err(unwritable)?;
        let probe = dir.join(format!(".recast-probe-{}", job.id));
        fs::write(&probe, b"").map_err(unwritable)?;
        let _ = fs::remove_file(&probe);
        Ok(dir)
    }

    /// Install the engine on a helper thread so a cancelled job stops waiting at once.
    /// The install itself carries on (another job, or the next drop, will want it).
    fn ensure_engine(&self, engine: &str, cancel: &CancelToken, progress: &mut dyn FnMut(&str, Option<f32>)) -> Result<(), Failure> {
        enum Msg {
            Pct(f32),
            Done(Result<(), String>),
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let host = self.host.clone();
        let id = engine.to_string();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let r = host.ensure(&id, &mut |p| {
                let _ = tx2.send(Msg::Pct(p));
            });
            let _ = tx.send(Msg::Done(r));
        });
        loop {
            if cancel.is_cancelled() {
                return Err(Failure::new(ErrorCode::Cancelled, "Cancelled."));
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Pct(p)) => progress("waiting_engine", Some(p)),
                Ok(Msg::Done(Ok(()))) => return Ok(()),
                Ok(Msg::Done(Err(e))) => {
                    return Err(Failure::new(ErrorCode::EngineInstallFailed, format!("Couldn't download the converter: {e}")))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Failure::new(ErrorCode::EngineInstallFailed, "The converter download stopped unexpectedly."))
                }
            }
        }
    }

    fn run_job(&self, job: &Job, cancel: &CancelToken, progress: &mut dyn FnMut(&str, Option<f32>)) -> Result<Finished, Failure> {
        use ErrorCode::*;
        let settings = (self.settings)();
        let preset = self.catalog.get(&job.preset_id).ok_or_else(|| Failure::new(EngineFailed, "This conversion no longer exists."))?;
        let first = &job.inputs[0];

        // 1. Sources readable (SOURCE_UNREADABLE)
        let mut total_size = 0u64;
        for p in &job.inputs {
            let f = fs::File::open(p).map_err(|_| Failure::new(SourceUnreadable, format!("Couldn't read {}.", file_name(p))))?;
            total_size += f.metadata().map(|m| m.len()).unwrap_or(0);
        }

        // 2. Destination writable (DEST_UNWRITABLE)
        let dest = self.dest_dir(&settings, job)?;

        // 3. Engine present (ENGINE_INSTALL_FAILED)
        if !self.host.is_installed(&preset.engine) {
            progress("waiting_engine", Some(0.0));
        }
        self.ensure_engine(&preset.engine, cancel, progress)?;
        let program = self
            .host
            .bin(&preset.engine, &preset.bin)
            .ok_or_else(|| Failure::new(EngineInstallFailed, "The converter is missing its program. Reinstall it from Library."))?;
        progress("running", Some(0.0));

        // 4. Facts for progress, Shrink and the disk check
        let probe = if preset.progress == ProgressKind::Ffmpeg || preset.uses("video_kbps") { self.host.probe(first) } else { None };
        let duration = probe.as_ref().and_then(|p| p.duration_s);
        let has_audio = probe.as_ref().map(|p| p.has_audio).unwrap_or(true);
        let pages = if preset.engine == "ghostscript" { self.host.pdf_pages(first) } else { None };
        let mut vars = settings.to_vars();
        vars.insert("reader".into(), pandoc_reader(&ext_of(first)));

        // 5. Shrink preconditions (ALREADY_SMALL, TARGET_TOO_SMALL)
        if preset.uses("video_kbps") {
            let target = settings.shrink.target_mb;
            if total_size <= target as u64 * MB {
                return Ok(Finished::Skipped(format!("{} is already under {target} MB.", file_name(first))));
            }
            let d = duration.ok_or_else(|| Failure::new(EngineFailed, "Couldn't read the video's length."))?;
            let kbps = math::shrink_video_kbps(target, d, has_audio).map_err(|min| {
                Failure::new(TargetTooSmall, format!("{target} MB is too small for this video. The smallest it can get is about {} MB.", min.ceil() as u64))
            })?;
            vars.insert("video_kbps".into(), kbps.to_string());
        }

        // 6. Disk space (DISK_FULL)
        let need = math::required_free(math::estimate_bytes(preset, total_size, duration, pages, settings.shrink.target_mb));
        if let Some(free) = platform::free_space(&dest) {
            if free < need {
                return Err(Failure::new(DiskFull, format!("Not enough disk space: about {} MB is needed.", need / MB + 1)));
            }
        }

        // 7. Hidden temp dir on the destination volume
        let tmp = tmp_dir_for(&dest, &job.id, &self.scratch)?;
        fs::create_dir_all(&tmp).map_err(|e| Failure::new(DestUnwritable, format!("Couldn't create a work folder: {e}")))?;
        let _guard = TmpGuard(tmp.clone());
        let _ = platform::set_hidden(&tmp);
        let _ = self.history.lock().unwrap().set_temp(&job.id, Some(&tmp.to_string_lossy()));

        let out_ext = preset.out_ext_for(first);
        // A name is taken if it exists, or if one of its extra outputs (e.g. Markdown's
        // "<stem>_media" folder) does: the .md links to that folder by name.
        let base_vars = vars.clone();
        let taken = |p: &Path| -> bool {
            if p.exists() {
                return true;
            }
            let stem = if preset.out_naming.is_folder() { p.file_name() } else { p.file_stem() };
            let mut v = base_vars.clone();
            v.insert("final_stem".into(), stem.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            preset.extra_outputs.iter().any(|x| expand(x, &v).map(|n| dest.join(n).exists()).unwrap_or(false))
        };
        let planned = naming::final_path(&dest, first, &preset.out_naming, &out_ext, &taken);
        let final_stem = if preset.out_naming.is_folder() { planned.file_name() } else { planned.file_stem() };
        vars.insert("final_stem".into(), final_stem.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        if preset.out_naming.is_folder() {
            fs::create_dir(tmp.join("out")).map_err(|e| Failure::new(DestUnwritable, e.to_string()))?;
        }
        let long_input = job.inputs.iter().any(|p| p.as_os_str().len() > 200);
        let inputs = if preset.stage_inputs || long_input { stage_inputs(&job.inputs, &tmp)? } else { job.inputs.clone() };
        let in_dir = first.parent().unwrap_or(Path::new("."));
        let passes = build_command(preset, &BuildInput { inputs: &inputs, in_dir, out_ext: &out_ext, vars: &vars })
            .map_err(|e| Failure::new(SourceUnreadable, e))?;

        // 8. Run every pass
        let n = passes.len();
        for (i, args) in passes.into_iter().enumerate() {
            let spec = RunSpec {
                program: program.clone(),
                args,
                cwd: tmp.clone(),
                progress: preset.progress,
                duration_s: duration,
                stall: Duration::from_secs(settings.jobs.stall_timeout_s as u64),
                hard_cap: Duration::from_secs(settings.jobs.no_progress_cap_min as u64 * 60 * job.inputs.len() as u64),
            };
            let res = runner::run(&spec, cancel, &mut |p| progress("running", Some((i as f32 + p) / n as f32)))
                .map_err(|e| Failure::new(EngineFailed, format!("Couldn't start the converter: {e}")))?;
            match res.outcome {
                RunOutcome::Exited(code) if preset.ok_exit.contains(&code) => {}
                RunOutcome::Exited(_) => {
                    let code = if explain::is_disk_full(&res.tail) { DiskFull } else { EngineFailed };
                    return Err(Failure { error: JobError { code, message: explain::explain(&res.tail) }, tail: res.tail });
                }
                RunOutcome::Stalled => {
                    return Err(Failure {
                        error: JobError { code: Stalled, message: "The converter stopped making progress, so Recast stopped it.".into() },
                        tail: res.tail,
                    })
                }
                RunOutcome::Cancelled => return Err(Failure::new(Cancelled, "Cancelled.")),
            }
        }

        // 9. Check and finalize
        let produced = if preset.out_naming.is_folder() { tmp.join("out") } else { tmp.join(format!("out.{out_ext}")) };
        if !produced.exists() {
            return Err(Failure::new(EngineFailed, "The converter finished but didn't write a file."));
        }
        if preset.discard_if_not_smaller && fs::metadata(&produced).map(|m| m.len()).unwrap_or(u64::MAX) >= total_size {
            return Ok(Finished::Skipped(format!("{} is already as small as it gets.", file_name(first))));
        }
        let final_path = finalize(&produced, &dest, first, preset, &out_ext, &taken)?;
        for extra in &preset.extra_outputs {
            let name = expand(extra, &vars).map_err(|e| Failure::new(EngineFailed, e))?;
            let src = tmp.join(&name);
            if src.exists() {
                let target = naming::unique_path(&dest, &name, None, &|p| p.exists());
                move_with_retry(&src, &target).map_err(map_move_err)?;
            }
        }
        Ok(Finished::Done(final_path))
    }
}

/// `<dest>\.recast-<id>` normally; engines can't use a working directory past
/// MAX_PATH, so long destinations use the scratch root when it's on the same volume.
fn tmp_dir_for(dest: &Path, id: &str, scratch: &Path) -> Result<PathBuf, Failure> {
    let local = dest.join(format!(".recast-{id}"));
    if local.as_os_str().len() < 240 {
        return Ok(local);
    }
    let drive = |p: &Path| match p.components().next() {
        Some(Component::Prefix(pre)) => Some(pre.as_os_str().to_string_lossy().to_ascii_uppercase()),
        _ => None,
    };
    if drive(dest).is_some() && drive(dest) == drive(scratch) {
        Ok(scratch.join(format!(".recast-{id}")))
    } else {
        Err(Failure::new(ErrorCode::DestUnwritable, "This folder's path is too long for Recast. Move the file somewhere with a shorter path."))
    }
}

/// Hard-link (or copy, across volumes) inputs into the temp dir as in-N.<ext>.
fn stage_inputs(inputs: &[PathBuf], tmp: &Path) -> Result<Vec<PathBuf>, Failure> {
    inputs
        .iter()
        .enumerate()
        .map(|(i, src)| {
            let ext = ext_of(src);
            let name = if ext.is_empty() { format!("in-{}", i + 1) } else { format!("in-{}.{ext}", i + 1) };
            let dst = tmp.join(&name);
            fs::hard_link(src, &dst)
                .or_else(|_| fs::copy(src, &dst).map(|_| ()))
                .map_err(|e| Failure::new(ErrorCode::SourceUnreadable, format!("Couldn't read {}: {e}", file_name(src))))?;
            Ok(PathBuf::from(name))
        })
        .collect()
}

fn finalize(produced: &Path, dest: &Path, first: &Path, preset: &Preset, out_ext: &str, taken: &dyn Fn(&Path) -> bool) -> Result<PathBuf, Failure> {
    for _ in 0..100 {
        // Re-check collisions at rename time; move_no_replace makes the check race-free.
        let target = naming::final_path(dest, first, &preset.out_naming, out_ext, taken);
        match move_with_retry(produced, &target) {
            Ok(()) => return Ok(target),
            Err(e) if platform::is_already_exists(&e) => continue,
            Err(e) => return Err(map_move_err(e)),
        }
    }
    Err(Failure::new(ErrorCode::DestUnwritable, "Couldn't find a free file name."))
}

fn map_move_err(e: io::Error) -> Failure {
    if platform::is_transient_lock(&e) {
        Failure::new(ErrorCode::DestLocked, "Another program is holding the output file. Try again in a moment.")
    } else {
        Failure::new(ErrorCode::DestUnwritable, format!("Couldn't save the result: {e}"))
    }
}

/// Antivirus/OneDrive/Dropbox briefly lock new files: retry 100 ms, doubling, ≤ 5 s total.
fn move_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut wait = 100u64;
    let mut waited = 0u64;
    loop {
        match platform::move_no_replace(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if platform::is_transient_lock(&e) && waited < 5000 => {
                let d = wait.min(5000 - waited);
                std::thread::sleep(Duration::from_millis(d));
                waited += d;
                wait *= 2;
            }
            Err(e) => return Err(e),
        }
    }
}

impl EngineHost for crate::engines::Engines {
    fn is_installed(&self, engine: &str) -> bool {
        self.is_ready(engine)
    }
    fn ensure(&self, engine: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        crate::engines::Engines::ensure(self, engine, progress)
    }
    fn bin(&self, engine: &str, bin: &str) -> Option<PathBuf> {
        crate::engines::Engines::bin(self, engine, bin)
    }
    fn probe(&self, path: &Path) -> Option<Probe> {
        let ffprobe = crate::engines::Engines::bin(self, "ffmpeg", "ffprobe")?;
        let args = [
            std::ffi::OsStr::new("-v"),
            std::ffi::OsStr::new("error"),
            std::ffi::OsStr::new("-print_format"),
            std::ffi::OsStr::new("json"),
            std::ffi::OsStr::new("-show_format"),
            std::ffi::OsStr::new("-show_streams"),
            path.as_os_str(),
        ];
        let out = crate::proc::run_capture(&ffprobe, args, Duration::from_secs(15))?;
        crate::detect::parse_ffprobe_json(&out)
    }
    fn pdf_pages(&self, path: &Path) -> Option<u32> {
        let qpdf = crate::engines::Engines::bin(self, "qpdf", "qpdf")?;
        let out = crate::proc::run_capture(&qpdf, [std::ffi::OsStr::new("--show-npages"), path.as_os_str()], Duration::from_secs(15))?;
        out.trim().parse().ok()
    }
}
```

Also create `jobs/notify.rs` containing only `//! Filled in by Task 15.` so the module compiles.

- [ ] **Step 3: Run the tests**

Run: `cargo test jobs::`
Expected: all pass (11 manager tests plus the earlier jobs tests). If a test hangs, the `Collect::wait_for` assertion message prints every event seen; read it before changing code.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(jobs): job manager with preconditions, temp dirs, atomic finalize and retries"
```

---

### Task 15: Notifications

**Files:**
- Modify: `src-tauri/src/jobs/notify.rs`

- [ ] **Step 1: Write the failing tests and implementation** — `jobs/notify.rs`

```rust
//! Done/failed notifications; one combined notification for batches of 3+ (spec §4.3).
use std::collections::HashMap;

use super::JobEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

#[derive(Default)]
struct Tally {
    finished: usize,
    ok: usize,
    failed: usize,
}

#[derive(Default)]
pub struct Notifier {
    batches: HashMap<String, Tally>,
}

fn base(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
}

impl Notifier {
    pub fn on_event(&mut self, ev: &JobEvent) -> Option<Notification> {
        let status = ev.status.as_str();
        if !matches!(status, "done" | "skipped" | "failed" | "cancelled") {
            return None;
        }
        if ev.batch_size >= 3 {
            let t = self.batches.entry(ev.batch_id.clone()).or_default();
            t.finished += 1;
            match status {
                "done" | "skipped" => t.ok += 1,
                "failed" => t.failed += 1,
                _ => {}
            }
            if t.finished < ev.batch_size {
                return None;
            }
            let t = self.batches.remove(&ev.batch_id).expect("tally exists");
            if t.ok == 0 && t.failed == 0 {
                return None;
            }
            let mut title = format!("{} file{} converted", t.ok, if t.ok == 1 { "" } else { "s" });
            if t.failed > 0 {
                title.push_str(&format!(" · {} failed", t.failed));
            }
            return Some(Notification { title, body: "Open Recast to see the results.".into() });
        }
        let message = || ev.error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
        match status {
            "done" => Some(Notification {
                title: format!("{} ready", ev.out_path.as_deref().map(base).unwrap_or_default()),
                body: format!("{} · from {}", ev.preset_label, ev.file),
            }),
            "skipped" => Some(Notification { title: "Nothing to do".into(), body: message() }),
            "failed" => Some(Notification { title: format!("Couldn't convert {}", ev.file), body: message() }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{ErrorCode, JobError};

    fn ev(batch: &str, size: usize, status: &str) -> JobEvent {
        JobEvent {
            job_id: "j".into(),
            batch_id: batch.into(),
            batch_size: size,
            preset_id: "video-mp3".into(),
            preset_label: "MP3".into(),
            file: "holiday.mov".into(),
            status: status.into(),
            pct: None,
            eta_s: None,
            out_path: (status == "done").then(|| r"C:\v\holiday.mp3".to_string()),
            error: (status == "failed").then(|| JobError { code: ErrorCode::EngineFailed, message: "The file looks damaged or incomplete.".into() }),
        }
    }

    #[test]
    fn single_jobs() {
        let mut n = Notifier::default();
        assert_eq!(n.on_event(&ev("b", 1, "running")), None);
        assert_eq!(n.on_event(&ev("b", 1, "done")).unwrap().title, "holiday.mp3 ready");
        let f = n.on_event(&ev("c", 1, "failed")).unwrap();
        assert_eq!(f.title, "Couldn't convert holiday.mov");
        assert_eq!(f.body, "The file looks damaged or incomplete.");
        assert_eq!(n.on_event(&ev("d", 1, "cancelled")), None);
    }

    #[test]
    fn batches_of_three_or_more_notify_once() {
        let mut n = Notifier::default();
        assert_eq!(n.on_event(&ev("b", 3, "done")), None);
        assert_eq!(n.on_event(&ev("b", 3, "failed")), None);
        assert_eq!(n.on_event(&ev("b", 3, "done")).unwrap().title, "2 files converted · 1 failed");
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test jobs::notify`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(jobs): completion notifications with batch summaries"
```

---

### Task 16: Tauri wiring — state, commands, events, tray, logging

**Files:**
- Create: `src-tauri/src/pad_model.rs`, `src-tauri/src/app_state.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write `pad_model.rs` with a test**

```rust
//! inspect(paths) → PadModel (spec §6 data flow).
use serde::Serialize;

use crate::detect::{self, FileInfo, Kind};
use crate::engines::install::EngineStatus;
use crate::engines::Engines;
use crate::jobs::EngineHost;
use crate::presets::available::{available, Caps, FileFacts};
use crate::presets::Catalog;

#[derive(Serialize, Clone, Debug)]
pub struct PresetView {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub engine: String,
    pub engine_name: String,
    pub engine_status: EngineStatus,
    pub min_files: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct PadModel {
    pub files: Vec<FileInfo>,
    pub presets: Vec<PresetView>,
    pub open_in_recast: bool,
}

pub fn build(catalog: &Catalog, engines: &Engines, paths: &[String]) -> Result<PadModel, String> {
    let mut files = Vec::new();
    for p in paths {
        let path = std::path::absolute(p).map_err(|e| e.to_string())?;
        if path.is_dir() {
            continue; // folders are a v1 non-goal (spec §3)
        }
        let mut info = detect::detect(&path, &|q| engines.probe(q)).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        if info.kind == Kind::Pdf {
            info.pages = engines.pdf_pages(&path);
        }
        files.push(info);
    }
    let facts: Vec<FileFacts> = files.iter().map(FileFacts::from).collect();
    let caps = engines.magick_caps();
    let avail = available(catalog, &facts, &Caps { magick: caps.as_ref() });
    let presets = avail
        .presets
        .iter()
        .map(|p| PresetView {
            id: p.id.clone(),
            label: p.label.clone(),
            description: p.description.clone(),
            icon: p.icon.clone(),
            engine: p.engine.clone(),
            engine_name: engines.manifest().get(&p.engine).map(|e| e.name.clone()).unwrap_or_else(|| p.engine.clone()),
            engine_status: engines.status(&p.engine),
            min_files: p.min_files,
        })
        .collect();
    Ok(PadModel { files, presets, open_in_recast: avail.open_in_recast })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::install::HttpFetcher;
    use crate::engines::manifest::{Manifest, BUNDLED};
    use crate::presets::BUNDLED as PRESETS;

    #[test]
    fn png_gets_image_presets_with_engine_status() {
        let d = tempfile::tempdir().unwrap();
        let engines = Engines::new(Manifest::parse(BUNDLED).unwrap(), d.path().join("engines"), d.path().join("7z.exe"), Box::new(HttpFetcher));
        let catalog = Catalog::parse(PRESETS).unwrap();
        let png = d.path().join("a.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R']).unwrap();
        let pdf = d.path().join("b.pdf");
        std::fs::write(&pdf, b"%PDF-1.7\n").unwrap();

        let m = build(&catalog, &engines, &[png.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(m.files[0].kind, Kind::Image);
        assert_eq!(m.presets[0].id, "image-jpg");
        assert_eq!(m.presets[0].engine_name, "ImageMagick");
        assert_eq!(m.presets[0].engine_status, EngineStatus::NotInstalled);

        let mixed = build(&catalog, &engines, &[png.to_string_lossy().into_owned(), pdf.to_string_lossy().into_owned()]).unwrap();
        assert!(mixed.presets.is_empty() && mixed.open_in_recast);
    }
}
```

Run: `cargo test pad_model::` (after adding `pub mod pad_model;` to `lib.rs`)
Expected: 1 passed.

- [ ] **Step 2: Write `logging.rs`**

```rust
//! Rotating log at %LOCALAPPDATA%\Recast\logs (5 × 1 MB). Paths only, never file contents.
use std::path::Path;

use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};

pub fn init(dir: &Path) {
    let started = Logger::try_with_str("info").and_then(|l| {
        l.log_to_file(FileSpec::default().directory(dir).basename("recast").suppress_timestamp())
            .append()
            .rotate(Criterion::Size(1_000_000), Naming::Numbers, Cleanup::KeepLogFiles(4))
            .start()
    });
    match started {
        Ok(handle) => {
            Box::leak(Box::new(handle));
        }
        Err(e) => eprintln!("logging disabled: {e}"),
    }
}
```

- [ ] **Step 3: Write `ipc.rs`**

```rust
//! Typed IPC (spec §6.11), mirrored in src/shared/ipc.ts.
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::engines::EngineRow;
use crate::history::JobRecord;
use crate::jobs::notify::Notifier;
use crate::jobs::{Events, JobEvent};
use crate::pad_model::{self, PadModel};
use crate::settings::{self, Settings};

/// Sends job events to every webview and raises notifications.
pub struct TauriEvents {
    app: AppHandle,
    notifier: Mutex<Notifier>,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app, notifier: Mutex::new(Notifier::default()) }
    }
}

impl Events for TauriEvents {
    fn emit(&self, ev: &JobEvent) {
        let name = match ev.status.as_str() {
            "queued" => "job://queued",
            "done" | "skipped" => "job://done",
            "failed" => "job://failed",
            "cancelled" => "job://cancelled",
            _ => "job://progress",
        };
        if let Err(e) = self.app.emit(name, ev) {
            log::warn!("emit {name}: {e}");
        }
        if let Some(n) = self.notifier.lock().unwrap().on_event(ev) {
            if let Err(e) = self.app.notification().builder().title(&n.title).body(&n.body).show() {
                log::warn!("notification: {e}");
            }
        }
    }
}

#[derive(Serialize)]
pub struct JobListItem {
    #[serde(flatten)]
    pub record: JobRecord,
    pub preset_label: String,
}

#[derive(Serialize)]
pub struct Notice {
    pub name: String,
    pub licence: String,
    pub homepage: String,
    pub source_url: Option<String>,
    pub licence_file: Option<String>,
    /// Shipped inside Recast (7-Zip) rather than downloaded.
    pub bundled: bool,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn inspect(state: State<'_, AppState>, paths: Vec<String>) -> Result<PadModel, String> {
    let (catalog, engines) = (state.catalog.clone(), state.engines.clone());
    blocking(move || pad_model::build(&catalog, &engines, &paths)).await
}

#[tauri::command]
pub fn start_jobs(state: State<'_, AppState>, preset_id: String, paths: Vec<String>) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    state.jobs.start_jobs(&preset_id, &paths)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: String) {
    state.jobs.cancel(&id);
}

#[tauri::command]
pub fn retry_job(state: State<'_, AppState>, id: String, fallback: bool) -> Result<Vec<String>, String> {
    state.jobs.retry(&id, fallback)
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>, page: u32) -> Result<Vec<JobListItem>, String> {
    let records = state.history.lock().unwrap().list(page, 50).map_err(|e| e.to_string())?;
    Ok(records
        .into_iter()
        .map(|record| JobListItem { preset_label: state.catalog.get(&record.preset_id).map(|p| p.label.clone()).unwrap_or_default(), record })
        .collect())
}

#[tauri::command]
pub fn remove_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.is_active(&id) {
        return Err("Cancel the job before removing it.".into());
    }
    state.history.lock().unwrap().remove(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engines_list(state: State<'_, AppState>) -> Result<Vec<EngineRow>, String> {
    let engines = state.engines.clone();
    blocking(move || Ok(engines.list())).await
}

const ENGINE_BUSY: &str = "A conversion is using this converter. Try again when it finishes.";

#[tauri::command]
pub fn engine_install(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.engine_in_use(&id) {
        return Err(ENGINE_BUSY.into());
    }
    let engines = state.engines.clone();
    // Progress/done/failed reach the UI through the Engines listener (app_state.rs).
    std::thread::spawn(move || {
        let _ = engines.install(&id, &mut |_| {});
    });
    Ok(())
}

#[tauri::command]
pub async fn engine_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.engine_in_use(&id) {
        return Err(ENGINE_BUSY.into());
    }
    let engines = state.engines.clone();
    blocking(move || engines.remove(&id)).await
}

#[tauri::command]
pub async fn engine_verify(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let engines = state.engines.clone();
    blocking(move || engines.verify(&id)).await
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().settings.clone()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, patch: serde_json::Value) -> Result<Settings, String> {
    let mut current = state.settings.lock().unwrap();
    let next = settings::apply_patch(&current, patch);
    for w in &next.warnings {
        log::warn!("settings: {w}");
    }
    settings::save(&state.paths.settings(), &next).map_err(|e| e.to_string())?;
    *current = next;
    let out = current.settings.clone();
    drop(current); // pump reads settings through the same mutex
    state.jobs.pump(); // a higher concurrency starts queued jobs now
    Ok(out)
}

#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

fn licence_file(dir: &std::path::Path) -> Option<String> {
    crate::fsutil::walk_files(dir)
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
            n.starts_with("LICENSE") || n.starts_with("COPYING") || n.starts_with("COPYRIGHT")
        })
        .min_by_key(|p| p.components().count())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn notices(state: State<'_, AppState>) -> Result<Vec<Notice>, String> {
    let engines = state.engines.clone();
    let sevenzip_licence = state.sevenzip_licence.clone();
    blocking(move || {
        let mut out: Vec<Notice> = engines
            .manifest()
            .engines
            .iter()
            .map(|e| Notice {
                name: e.name.clone(),
                licence: e.licence.clone(),
                homepage: e.homepage.clone(),
                source_url: e.source_url.clone(),
                licence_file: licence_file(&engines.root().join(&e.id)),
                bundled: false,
            })
            .collect();
        out.push(Notice {
            name: "7-Zip (7z.exe, 7z.dll)".into(),
            licence: "LGPL-2.1-or-later with BSD-3-Clause and the unRAR restriction".into(),
            homepage: "https://www.7-zip.org".into(),
            source_url: Some("https://www.7-zip.org/download.html".into()),
            licence_file: sevenzip_licence.exists().then(|| sevenzip_licence.to_string_lossy().into_owned()),
            bundled: true,
        });
        Ok(out)
    })
    .await
}
```

- [ ] **Step 4: Write `app_state.rs`**

```rust
//! Builds everything at startup and hands it to Tauri.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};

use crate::engines::install::HttpFetcher;
use crate::engines::manifest::{self, Manifest};
use crate::engines::{EngineEvent, Engines};
use crate::history::History;
use crate::ipc::TauriEvents;
use crate::jobs::{now, JobManager};
use crate::paths::Paths;
use crate::presets::{self, Catalog};
use crate::settings;

pub struct AppState {
    pub paths: Paths,
    pub settings: Arc<Mutex<settings::Loaded>>,
    pub catalog: Arc<Catalog>,
    pub engines: Arc<Engines>,
    pub history: Arc<Mutex<History>>,
    pub jobs: JobManager,
    pub sevenzip_licence: PathBuf,
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::from_env();
    std::fs::create_dir_all(&paths.local)?;
    std::fs::create_dir_all(&paths.roaming)?;
    crate::logging::init(&paths.logs());
    log::info!("Recast {} starting", app.package_info().version);

    let loaded = settings::load(&paths.settings());
    for w in &loaded.warnings {
        log::warn!("settings: {w}");
    }

    let manifest = Manifest::parse(manifest::BUNDLED)?;
    manifest.validate().map_err(|e| e.join("; "))?;
    let catalog = Arc::new(Catalog::parse(presets::BUNDLED)?);
    catalog.validate(&manifest.bins_map(), &settings::var_keys()).map_err(|e| e.join("; "))?;

    let sevenzip = app.path().resolve("bin/7z.exe", BaseDirectory::Resource)?;
    let sevenzip_licence = app.path().resolve("bin/7zip-License.txt", BaseDirectory::Resource)?;
    let engines = Arc::new(Engines::new(manifest, paths.engines(), sevenzip, Box::new(HttpFetcher)));
    let handle = app.handle().clone();
    engines.set_listener(Arc::new(move |e: EngineEvent| {
        let name = match &e {
            EngineEvent::Progress { .. } => "engine://progress",
            EngineEvent::Done { .. } => "engine://done",
            EngineEvent::Failed { .. } => "engine://failed",
        };
        let _ = handle.emit(name, &e);
    }));

    let history = Arc::new(Mutex::new(History::open(&paths.db())?));
    for tmp in history.lock().unwrap().recover_interrupted(now())? {
        match std::fs::remove_dir_all(&tmp) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => log::warn!("leftover {tmp}: {e}"),
            _ => log::info!("removed leftover temp dir {tmp}"),
        }
    }

    let settings = Arc::new(Mutex::new(loaded));
    let for_jobs = settings.clone();
    let jobs = JobManager::new(
        catalog.clone(),
        engines.clone(),
        history.clone(),
        Arc::new(move || for_jobs.lock().unwrap().settings.clone()),
        Arc::new(TauriEvents::new(app.handle().clone())),
        paths.local.join("tmp"),
    );

    app.manage(AppState { paths, settings, catalog, engines, history, jobs, sevenzip_licence });
    Ok(())
}
```

- [ ] **Step 5: Write `tray.rs`**

```rust
//! Tray icon: left-click opens the companion; menu has Open and Quit (spec §4.5).
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn show_companion(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("companion") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Recast", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("Recast")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_companion(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_companion(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
```

- [ ] **Step 6: Final `lib.rs`**

```rust
pub mod app_state;
pub mod detect;
pub mod engines;
pub mod fsutil;
pub mod history;
pub mod ipc;
pub mod jobs;
pub mod logging;
pub mod pad_model;
pub mod paths;
pub mod platform;
pub mod presets;
pub mod proc;
pub mod settings;
pub mod tray;

#[cfg(test)]
mod testutil;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first. A second copy would mark this copy's running jobs INTERRUPTED
        // and delete their temp dirs; instead it just focuses the running instance.
        // (Plan 3 extends this to forward Explorer selections.)
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| tray::show_companion(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app_state::setup(app)?;
            tray::build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the companion hides it; Recast stays in the tray (spec §4.5).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "companion" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::inspect,
            ipc::start_jobs,
            ipc::cancel_job,
            ipc::retry_job,
            ipc::list_jobs,
            ipc::remove_job,
            ipc::engines_list,
            ipc::engine_install,
            ipc::engine_remove,
            ipc::engine_verify,
            ipc::get_settings,
            ipc::set_settings,
            ipc::reveal,
            ipc::open_path,
            ipc::notices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
```

> If `show_menu_on_left_click` doesn't exist in the installed Tauri version, it's the older `menu_on_left_click`. If `reveal_item_in_dir` takes a `Path` rather than `String`, pass `PathBuf::from(path)`.

- [ ] **Step 7: Build, run the whole suite, and smoke-test the app**

```bash
npm run build
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```
Expected: all tests pass; clippy clean.

Then launch the app in the background and check it started:
```bash
npm run tauri dev
```
(run with `run_in_background`; wait ~60 s for the first compile.) Then:
```bash
cat "$LOCALAPPDATA/Recast/logs/"recast*.log | tail -5
```
Expected: a line `Recast 0.1.0 starting` and no `ERROR`. Stop the dev process afterwards.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(app): Tauri state, typed IPC commands, job/engine events, tray and logging"
```

---

### Task 17: Front-end types, IPC wrappers and the job reducer

**Files:**
- Create: `src/shared/types.ts`, `src/shared/ipc.ts`, `src/shared/format.ts`, `src/shared/format.test.ts`, `src/companion/jobs.ts`, `src/companion/jobs.test.ts`

- [ ] **Step 1: Write `src/shared/types.ts`** (mirror of the Rust types; keep in sync by hand)

```ts
export type Kind = "video" | "audio" | "image" | "pdf" | "text" | "unknown";

export interface FileInfo {
  path: string;
  name: string;
  kind: Kind;
  ext: string;
  format: string;
  size: number;
  duration_s: number | null;
  has_audio: boolean | null;
  pages: number | null;
}

export type EngineStatus =
  | { state: "not_installed" }
  | { state: "installing"; pct: number }
  | { state: "installed"; version: string; update_available: boolean }
  | { state: "damaged" };

export interface PresetView {
  id: string;
  label: string;
  description: string;
  icon: string;
  engine: string;
  engine_name: string;
  engine_status: EngineStatus;
  min_files: number;
}

export interface PadModel {
  files: FileInfo[];
  presets: PresetView[];
  open_in_recast: boolean;
}

export type JobStatus = "queued" | "waiting_engine" | "running" | "done" | "skipped" | "failed" | "cancelled" | "interrupted";

export interface JobError {
  code: string;
  message: string;
}

export interface JobEvent {
  job_id: string;
  batch_id: string;
  batch_size: number;
  preset_id: string;
  preset_label: string;
  file: string;
  status: JobStatus;
  pct: number | null;
  eta_s: number | null;
  out_path: string | null;
  error: JobError | null;
}

export interface JobRecord {
  id: string;
  preset_id: string;
  preset_label: string;
  inputs: string[];
  output: string | null;
  status: JobStatus;
  error_code: string | null;
  error_detail: string | null;
  stderr_tail: string | null;
  started_at: number;
  ended_at: number | null;
  temp_path: string | null;
}

export interface EngineRow {
  id: string;
  name: string;
  version: string;
  size: number;
  licence: string;
  homepage: string;
  source_url: string | null;
  status: EngineStatus;
  disk_bytes: number;
}

export type EngineEvent =
  | { kind: "progress"; id: string; pct: number }
  | { kind: "done"; id: string }
  | { kind: "failed"; id: string; error: string };

export interface Notice {
  name: string;
  licence: string;
  homepage: string;
  source_url: string | null;
  licence_file: string | null;
  bundled: boolean;
}

export interface Settings {
  version: number;
  triggers: { edge_handle: boolean; edge: "left" | "right" | "top" | "bottom"; edge_pos: number; desk_pad: boolean; shake: boolean };
  shell: { explorer_entry: boolean };
  output: { location: "next_to_original" | "folder"; folder: string; fallback_folder: string };
  quality: { mp3_kbps: number; aac_kbps: number; jpg_q: number; webp_q: number; avif_q: number };
  gif: { fps: number; width: number };
  shrink: { target_mb: number; image_px: number };
  jobs: { concurrency: number; stall_timeout_s: number; no_progress_cap_min: number };
  app: { start_with_windows: boolean; first_run_done: boolean };
}

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };
```

- [ ] **Step 2: Write `src/shared/ipc.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DeepPartial, EngineEvent, EngineRow, JobEvent, JobRecord, Notice, PadModel, Settings } from "./types";

export const JOB_EVENTS = ["job://queued", "job://progress", "job://done", "job://failed", "job://cancelled"] as const;
export const ENGINE_EVENTS = ["engine://progress", "engine://done", "engine://failed"] as const;

export const ipc = {
  inspect: (paths: string[]) => invoke<PadModel>("inspect", { paths }),
  startJobs: (presetId: string, paths: string[]) => invoke<string[]>("start_jobs", { presetId, paths }),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  retryJob: (id: string, fallback = false) => invoke<string[]>("retry_job", { id, fallback }),
  listJobs: (page = 0) => invoke<JobRecord[]>("list_jobs", { page }),
  removeJob: (id: string) => invoke<void>("remove_job", { id }),
  enginesList: () => invoke<EngineRow[]>("engines_list"),
  engineInstall: (id: string) => invoke<void>("engine_install", { id }),
  engineRemove: (id: string) => invoke<void>("engine_remove", { id }),
  engineVerify: (id: string) => invoke<boolean>("engine_verify", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: DeepPartial<Settings>) => invoke<Settings>("set_settings", { patch }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  open: (path: string) => invoke<void>("open_path", { path }),
  notices: () => invoke<Notice[]>("notices"),
};

async function listenAll<T>(names: readonly string[], cb: (payload: T) => void): Promise<UnlistenFn> {
  const offs = await Promise.all(names.map((n) => listen<T>(n, (e) => cb(e.payload))));
  return () => offs.forEach((off) => off());
}

export const onJobEvent = (cb: (e: JobEvent) => void) => listenAll(JOB_EVENTS, cb);
export const onEngineEvent = (cb: (e: EngineEvent) => void) => listenAll(ENGINE_EVENTS, cb);
```

- [ ] **Step 3: Write the failing tests** — `src/shared/format.test.ts` and `src/companion/jobs.test.ts`

```ts
// src/shared/format.test.ts
import { baseName, formatBytes, formatEta } from "./format";

test("formatBytes", () => {
  expect(formatBytes(500)).toBe("500 B");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(25 * 1024 * 1024)).toBe("25.0 MB");
  expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
});

test("formatEta", () => {
  expect(formatEta(null)).toBe("");
  expect(formatEta(42)).toBe("42s left");
  expect(formatEta(125)).toBe("2m 05s left");
  expect(formatEta(3700)).toBe("1h 01m left");
});

test("baseName", () => {
  expect(baseName("C:\\v\\holiday.mp3")).toBe("holiday.mp3");
  expect(baseName("C:/v/a b.png")).toBe("a b.png");
});
```

```ts
// src/companion/jobs.test.ts
import type { JobEvent, JobRecord } from "../shared/types";
import { applyEvent, empty, remove, seed, split, withRecord } from "./jobs";

const ev = (over: Partial<JobEvent>): JobEvent => ({
  job_id: "j1", batch_id: "b", batch_size: 1, preset_id: "video-mp3", preset_label: "MP3", file: "holiday.mov",
  status: "queued", pct: null, eta_s: null, out_path: null, error: null, ...over,
});

const rec = (over: Partial<JobRecord>): JobRecord => ({
  id: "old", preset_id: "image-png", preset_label: "PNG", inputs: ["C:\\p\\a.jpg", "C:\\p\\b.jpg"], output: null,
  status: "done", error_code: null, error_detail: null, stderr_tail: null, started_at: 1, ended_at: 2, temp_path: null, ...over,
});

test("seed builds views from history", () => {
  const s = seed([rec({})]);
  expect(s.order).toEqual(["old"]);
  expect(s.byId.old.file).toBe("a.jpg +1");
  expect(s.byId.old.presetLabel).toBe("PNG");
});

test("events add new jobs on top and update existing ones", () => {
  let s = seed([rec({})]);
  s = applyEvent(s, ev({}));
  expect(s.order).toEqual(["j1", "old"]);
  s = applyEvent(s, ev({ status: "running", pct: 0.5, eta_s: 10 }));
  expect(s.byId.j1.pct).toBe(0.5);
  s = applyEvent(s, ev({ status: "done", pct: 1, out_path: "C:\\v\\holiday.mp3" }));
  expect(s.order).toEqual(["j1", "old"]);
  expect(s.byId.j1.outPath).toBe("C:\\v\\holiday.mp3");
});

test("split separates active jobs from history", () => {
  let s = applyEvent(empty, ev({ job_id: "a", status: "running" }));
  s = applyEvent(s, ev({ job_id: "b", status: "failed", error: { code: "ENGINE_FAILED", message: "x" } }));
  const { active, history } = split(s);
  expect(active.map((j) => j.id)).toEqual(["a"]);
  expect(history.map((j) => j.id)).toEqual(["b"]);
});

test("withRecord adds the stderr tail; remove drops a job", () => {
  let s = applyEvent(empty, ev({ status: "failed", error: { code: "ENGINE_FAILED", message: "x" } }));
  s = withRecord(s, rec({ id: "j1", status: "failed", stderr_tail: "boom" }));
  expect(s.byId.j1.stderrTail).toBe("boom");
  s = remove(s, "j1");
  expect(s.order).toEqual([]);
});
```

Run: `npm test`
Expected: FAIL (modules not found).

- [ ] **Step 4: Implement `src/shared/format.ts` and `src/companion/jobs.ts`**

```ts
// src/shared/format.ts
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i++;
  } while (v >= 1024 && i < units.length - 1);
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatEta(s: number | null): string {
  if (s == null || !Number.isFinite(s)) return "";
  const r = Math.max(0, Math.round(s));
  if (r < 60) return `${r}s left`;
  const m = Math.floor(r / 60);
  if (m < 60) return `${m}m ${String(r % 60).padStart(2, "0")}s left`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m left`;
}

export function baseName(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}
```

```ts
// src/companion/jobs.ts
import { baseName } from "../shared/format";
import type { JobError, JobEvent, JobRecord, JobStatus } from "../shared/types";

export interface JobView {
  id: string;
  presetId: string;
  presetLabel: string;
  file: string;
  status: JobStatus;
  pct: number | null;
  etaS: number | null;
  outPath: string | null;
  error: JobError | null;
  stderrTail: string | null;
}

export interface JobsState {
  order: string[];
  byId: Record<string, JobView>;
}

export const empty: JobsState = { order: [], byId: {} };
export const ACTIVE: JobStatus[] = ["queued", "waiting_engine", "running"];

export function fromRecord(r: JobRecord): JobView {
  const first = r.inputs[0] ? baseName(r.inputs[0]) : "";
  return {
    id: r.id,
    presetId: r.preset_id,
    presetLabel: r.preset_label,
    file: r.inputs.length > 1 ? `${first} +${r.inputs.length - 1}` : first,
    status: r.status,
    pct: null,
    etaS: null,
    outPath: r.output,
    error: r.error_code ? { code: r.error_code, message: r.error_detail ?? "" } : null,
    stderrTail: r.stderr_tail,
  };
}

export function seed(records: JobRecord[]): JobsState {
  return { order: records.map((r) => r.id), byId: Object.fromEntries(records.map((r) => [r.id, fromRecord(r)])) };
}

export function applyEvent(s: JobsState, e: JobEvent): JobsState {
  const prev = s.byId[e.job_id];
  const next: JobView = {
    id: e.job_id,
    presetId: e.preset_id,
    presetLabel: e.preset_label,
    file: e.file,
    status: e.status,
    pct: e.pct ?? (ACTIVE.includes(e.status) ? prev?.pct ?? null : null),
    etaS: e.eta_s,
    outPath: e.out_path ?? prev?.outPath ?? null,
    error: e.error,
    stderrTail: prev?.stderrTail ?? null,
  };
  return { order: prev ? s.order : [e.job_id, ...s.order], byId: { ...s.byId, [e.job_id]: next } };
}

export function withRecord(s: JobsState, r: JobRecord): JobsState {
  const prev = s.byId[r.id];
  if (!prev) return s;
  return { ...s, byId: { ...s.byId, [r.id]: { ...prev, stderrTail: r.stderr_tail } } };
}

export function remove(s: JobsState, id: string): JobsState {
  const byId = { ...s.byId };
  delete byId[id];
  return { order: s.order.filter((x) => x !== id), byId };
}

export function split(s: JobsState): { active: JobView[]; history: JobView[] } {
  const all = s.order.map((id) => s.byId[id]);
  return { active: all.filter((j) => ACTIVE.includes(j.status)), history: all.filter((j) => !ACTIVE.includes(j.status)) };
}
```

- [ ] **Step 5: Run the tests**

Run: `npm test`
Expected: 7 passed.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ui): typed IPC wrappers, formatting helpers and job list reducer"
```

---

### Task 18: Companion shell and the Activity view

**Files:**
- Create: `src/companion/App.tsx`, `src/companion/Activity.tsx`, `src/companion/companion.css`, `public/icon.svg`
- Modify: `src/main.tsx`

The Library and Settings views are created in Task 19; for this task, create them as stubs (`export function Library() { return null; }` and `export function SettingsView() { return null; }` in `src/companion/Library.tsx` and `src/companion/Settings.tsx`).

- [ ] **Step 1: Copy the icon for the sidebar**

```bash
mkdir -p public && cp assets/icon/recast-icon.svg public/icon.svg
```

- [ ] **Step 2: Write `src/main.tsx` and `src/companion/App.tsx`**

```tsx
// src/main.tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./companion/App";
import "./companion/companion.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

```tsx
// src/companion/App.tsx
import { useState } from "react";
import { Activity } from "./Activity";
import { Library } from "./Library";
import { SettingsView } from "./Settings";

const SECTIONS = [
  { id: "activity", label: "Activity" },
  { id: "library", label: "Library" },
  { id: "settings", label: "Settings" },
] as const;
type Section = (typeof SECTIONS)[number]["id"];

export function App() {
  const [section, setSection] = useState<Section>("activity");
  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Sections">
        <div className="brand">
          <img src="/icon.svg" alt="" width={22} height={22} /> Recast
        </div>
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            className={s.id === section ? "nav active" : "nav"}
            aria-current={s.id === section ? "page" : undefined}
            onClick={() => setSection(s.id)}
          >
            {s.label}
          </button>
        ))}
      </nav>
      {/* All views stay mounted so Activity keeps receiving job events. */}
      <main className="content" hidden={section !== "activity"}>
        <Activity />
      </main>
      <main className="content" hidden={section !== "library"}>
        <Library />
      </main>
      <main className="content" hidden={section !== "settings"}>
        <SettingsView />
      </main>
    </div>
  );
}
```

- [ ] **Step 3: Write `src/companion/Activity.tsx`**

```tsx
import { useEffect, useReducer, useState, type Dispatch } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ipc, onJobEvent } from "../shared/ipc";
import { formatBytes, formatEta } from "../shared/format";
import type { JobEvent, JobRecord, PadModel } from "../shared/types";
import { applyEvent, empty, remove, seed, split, withRecord, type JobsState, type JobView } from "./jobs";

type Action =
  | { type: "seed"; records: JobRecord[] }
  | { type: "event"; e: JobEvent }
  | { type: "record"; r: JobRecord }
  | { type: "remove"; id: string };

function reducer(s: JobsState, a: Action): JobsState {
  switch (a.type) {
    case "seed":
      return seed(a.records);
    case "event":
      return applyEvent(s, a.e);
    case "record":
      return withRecord(s, a.r);
    case "remove":
      return remove(s, a.id);
  }
}

interface Dropped {
  paths: string[];
  pad: PadModel;
}

export function Activity() {
  const [jobs, dispatch] = useReducer(reducer, empty);
  const [dropped, setDropped] = useState<Dropped | null>(null);
  const [hovering, setHovering] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc.listJobs(0).then((records) => dispatch({ type: "seed", records })).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    const off = onJobEvent((e) => {
      dispatch({ type: "event", e });
      if (e.status === "failed") {
        ipc.listJobs(0).then((rs) => {
          const r = rs.find((x) => x.id === e.job_id);
          if (r) dispatch({ type: "record", r });
        });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent(async (ev) => {
      const p = ev.payload;
      if (p.type === "enter" || p.type === "over") setHovering(true);
      else if (p.type === "leave") setHovering(false);
      else if (p.type === "drop") {
        setHovering(false);
        if (p.paths.length === 0) return;
        try {
          setError(null);
          setDropped({ paths: p.paths, pad: await ipc.inspect(p.paths) });
        } catch (e) {
          setError(String(e));
        }
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  async function start(presetId: string) {
    if (!dropped) return;
    try {
      await ipc.startJobs(presetId, dropped.paths);
      setDropped(null);
    } catch (e) {
      setError(String(e));
    }
  }

  const { active, history } = split(jobs);

  return (
    <section className={hovering ? "activity drop-hover" : "activity"} aria-label="Activity">
      <div className="dropzone" aria-live="polite">
        {hovering ? "Release to see conversions" : "Drop files here to convert them"}
      </div>
      {error && <p className="error" role="alert">{error}</p>}
      {dropped && <PresetPicker dropped={dropped} onPick={start} onClose={() => setDropped(null)} />}

      {active.length > 0 && (
        <>
          <h2>Running</h2>
          <ul className="jobs">
            {active.map((j) => (
              <JobRow key={j.id} job={j} dispatch={dispatch} />
            ))}
          </ul>
        </>
      )}

      <h2>History</h2>
      {history.length === 0 ? (
        <p className="muted">Nothing converted yet.</p>
      ) : (
        <ul className="jobs">
          {history.map((j) => (
            <JobRow key={j.id} job={j} dispatch={dispatch} />
          ))}
        </ul>
      )}
    </section>
  );
}

function PresetPicker({ dropped, onPick, onClose }: { dropped: Dropped; onPick: (id: string) => void; onClose: () => void }) {
  const { pad } = dropped;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const one = pad.files.length === 1 ? pad.files[0] : null;
  const meta = one
    ? [one.format.toUpperCase(), formatBytes(one.size), one.pages ? `${one.pages} pages` : null].filter(Boolean).join(" · ")
    : `${formatBytes(pad.files.reduce((a, f) => a + f.size, 0))} in total`;
  return (
    <div className="picker" role="dialog" aria-label="Choose a conversion">
      <header>
        <div>
          <strong>{one ? one.name : `${pad.files.length} files`}</strong>
          <div className="muted">{meta}</div>
        </div>
        <button className="ghost" onClick={onClose} aria-label="Close">
          ✕
        </button>
      </header>
      {pad.presets.length === 0 ? (
        <p className="muted">
          {pad.open_in_recast
            ? "These files are different kinds. Drop each kind separately."
            : "Recast has no conversions for this file."}
        </p>
      ) : (
        <ul className="presets">
          {pad.presets.map((p) => (
            <li key={p.id}>
              <button onClick={() => onPick(p.id)} title={p.description}>
                <span className="label">{p.label}</span>
                <span className="desc">{p.description}</span>
                {p.engine_status.state !== "installed" && <span className="badge">Downloads {p.engine_name}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function statusLine(j: JobView): string {
  switch (j.status) {
    case "queued":
      return "Waiting to start";
    case "waiting_engine":
      return `Getting the converter${j.pct != null ? ` · ${Math.round(j.pct * 100)}%` : ""}`;
    case "running":
      return j.pct != null ? [`${Math.round(j.pct * 100)}%`, formatEta(j.etaS)].filter(Boolean).join(" · ") : "Working…";
    case "done":
      return j.outPath ? `Saved as ${j.outPath.split(/[\\/]/).pop()}` : "Done";
    case "skipped":
    case "failed":
      return j.error?.message ?? "Failed";
    case "cancelled":
      return "Cancelled";
    case "interrupted":
      return "Interrupted when Recast closed";
  }
}

function JobRow({ job: j, dispatch }: { job: JobView; dispatch: Dispatch<Action> }) {
  const [open, setOpen] = useState(false);
  const running = j.status === "queued" || j.status === "waiting_engine" || j.status === "running";
  const removeRow = () => ipc.removeJob(j.id).then(() => dispatch({ type: "remove", id: j.id }));
  return (
    <li className={`job ${j.status}`}>
      <div className="job-main">
        <div className="job-title">
          <strong>{j.file}</strong> <span className="muted">→ {j.presetLabel}</span>
        </div>
        <div className="job-status">{statusLine(j)}</div>
        {running && (
          <div className={j.pct == null ? "bar indeterminate" : "bar"} role="progressbar" aria-valuenow={j.pct == null ? undefined : Math.round(j.pct * 100)}>
            <div style={{ width: `${Math.round((j.pct ?? 0) * 100)}%` }} />
          </div>
        )}
        {open && j.status === "failed" && (
          <pre className="details">{[j.error?.code, j.stderrTail].filter(Boolean).join("\n\n")}</pre>
        )}
      </div>
      <div className="job-actions">
        {running && <button onClick={() => ipc.cancelJob(j.id)}>Cancel</button>}
        {j.status === "done" && j.outPath && (
          <>
            <button onClick={() => ipc.reveal(j.outPath!)}>Show in folder</button>
            <button onClick={() => ipc.open(j.outPath!)}>Open</button>
          </>
        )}
        {j.status === "failed" && (j.error?.code === "DEST_UNWRITABLE" || j.error?.code === "DEST_LOCKED") && (
          <button onClick={() => ipc.retryJob(j.id, true)}>Save to fallback folder</button>
        )}
        {(j.status === "failed" || j.status === "cancelled" || j.status === "interrupted") && (
          <button onClick={() => ipc.retryJob(j.id)}>Retry</button>
        )}
        {j.status === "failed" && <button onClick={() => setOpen(!open)}>{open ? "Hide details" : "Details"}</button>}
        {!running && (
          <button className="ghost" onClick={removeRow} aria-label="Remove from list">
            Remove
          </button>
        )}
      </div>
    </li>
  );
}
```

- [ ] **Step 4: Write `src/companion/companion.css`**

```css
:root {
  color-scheme: light dark;
  --bg: #f2f2f5;
  --panel: rgba(255, 255, 255, 0.72);
  --line: rgba(0, 0, 0, 0.08);
  --text: #1d1d1f;
  --muted: #6e6e73;
  --accent: #0a84ff;
  --danger: #d70015;
  font: 13px/1.45 "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  color: var(--text);
  background: var(--bg);
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #1c1c20;
    --panel: rgba(43, 43, 48, 0.78);
    --line: rgba(255, 255, 255, 0.09);
    --text: #f5f5f7;
    --muted: #a1a1a6;
    --danger: #ff6961;
  }
}
* { box-sizing: border-box; }
body { margin: 0; }
[hidden] { display: none !important; }
button {
  font: inherit; color: inherit; cursor: pointer;
  border: 1px solid var(--line); border-radius: 8px; background: var(--panel);
  padding: 4px 10px;
}
button:hover { border-color: var(--accent); }
button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
button.ghost { background: transparent; border-color: transparent; }
h2 { font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin: 18px 0 8px; }
.muted { color: var(--muted); }
.error { color: var(--danger); }

.shell { display: grid; grid-template-columns: 168px 1fr; height: 100vh; }
.sidebar { border-right: 1px solid var(--line); padding: 14px 10px; display: flex; flex-direction: column; gap: 4px; }
.brand { display: flex; align-items: center; gap: 8px; font-weight: 600; margin: 0 6px 14px; }
.nav { text-align: left; border-color: transparent; background: transparent; }
.nav.active { background: var(--panel); border-color: var(--line); font-weight: 600; }
.content { overflow: auto; padding: 18px 22px; }

.dropzone {
  border: 1.5px dashed var(--line); border-radius: 14px; padding: 26px; text-align: center; color: var(--muted);
  transition: border-color 0.15s, background 0.15s;
}
.drop-hover .dropzone { border-color: var(--accent); background: rgba(10, 132, 255, 0.08); color: var(--text); }

.picker { margin-top: 14px; border: 1px solid var(--line); border-radius: 14px; background: var(--panel); padding: 12px; backdrop-filter: blur(20px); }
.picker header { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 10px; }
.presets { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 8px; }
.presets button { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 2px; padding: 8px 10px; }
.presets .label { font-weight: 600; }
.presets .desc { color: var(--muted); font-size: 12px; }
.badge { font-size: 11px; color: var(--accent); }

.jobs { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.job { display: flex; gap: 12px; justify-content: space-between; border: 1px solid var(--line); border-radius: 10px; padding: 8px 10px; background: var(--panel); }
.job-main { min-width: 0; flex: 1; }
.job-title strong { overflow-wrap: anywhere; }
.job-status { color: var(--muted); font-size: 12px; }
.job.failed .job-status { color: var(--danger); }
.job-actions { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; justify-content: flex-end; }
.details { white-space: pre-wrap; font: 11px/1.4 Consolas, monospace; max-height: 180px; overflow: auto; background: rgba(0, 0, 0, 0.06); padding: 8px; border-radius: 6px; }

.bar { height: 4px; border-radius: 2px; background: var(--line); overflow: hidden; margin-top: 6px; }
.bar > div { height: 100%; background: var(--accent); transition: width 0.2s; }
.bar.indeterminate > div { width: 30% !important; animation: slide 1.2s ease-in-out infinite; }
@keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
@media (prefers-reduced-motion: reduce) { .bar.indeterminate > div { animation: none; } }

.rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.row { display: flex; justify-content: space-between; align-items: center; gap: 12px; border: 1px solid var(--line); border-radius: 10px; padding: 8px 10px; background: var(--panel); }
.field { display: grid; grid-template-columns: 200px 1fr; align-items: center; gap: 10px; margin: 6px 0; }
.field input[type="number"], .field input[type="text"], .field select { font: inherit; padding: 4px 6px; border-radius: 6px; border: 1px solid var(--line); background: var(--panel); color: inherit; max-width: 320px; }
```

- [ ] **Step 5: Type-check and test**

```bash
npm run build && npm test
```
Expected: build succeeds, tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ui): companion shell and Activity view with drop, presets and job list"
```

---

### Task 19: Library and Settings views

**Files:**
- Modify: `src/companion/Library.tsx`, `src/companion/Settings.tsx`

- [ ] **Step 1: Write `src/companion/Library.tsx`**

```tsx
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc, onEngineEvent } from "../shared/ipc";
import { formatBytes } from "../shared/format";
import type { EngineRow, Notice } from "../shared/types";

export function Library() {
  const [rows, setRows] = useState<EngineRow[]>([]);
  const [notices, setNotices] = useState<Notice[]>([]);
  const [messages, setMessages] = useState<Record<string, string>>({});

  const say = (id: string, msg: string | null) =>
    setMessages((m) => {
      const next = { ...m };
      if (msg) next[id] = msg;
      else delete next[id];
      return next;
    });

  const refresh = useCallback(() => {
    ipc.enginesList().then(setRows);
    ipc.notices().then(setNotices);
  }, []);

  useEffect(refresh, [refresh]);

  useEffect(() => {
    const off = onEngineEvent((e) => {
      if (e.kind === "progress") {
        setRows((rs) => rs.map((r) => (r.id === e.id ? { ...r, status: { state: "installing", pct: e.pct } } : r)));
      } else {
        say(e.id, e.kind === "failed" ? e.error : null);
        refresh();
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [refresh]);

  const verify = async (id: string) => {
    const ok = await ipc.engineVerify(id);
    say(id, ok ? "Verified: files match what was installed." : "Damaged: download it again to repair.");
    refresh();
  };

  const total = rows.reduce((a, r) => a + r.disk_bytes, 0);

  return (
    <section aria-label="Library">
      <h2>Converters</h2>
      <ul className="rows">
        {rows.map((r) => (
          <li key={r.id} className="row">
            <div>
              <strong>{r.name}</strong> <span className="muted">{r.version} · {r.licence}</span>
              <div className="muted">{describe(r)}</div>
              {messages[r.id] && <div className="muted" role="status">{messages[r.id]}</div>}
            </div>
            <div className="job-actions">
              {r.status.state === "not_installed" && <button onClick={() => ipc.engineInstall(r.id).catch((e) => say(r.id, String(e)))}>Download ({formatBytes(r.size)})</button>}
              {r.status.state === "damaged" && <button onClick={() => ipc.engineInstall(r.id).catch((e) => say(r.id, String(e)))}>Repair</button>}
              {r.status.state === "installed" && r.status.update_available && <button onClick={() => ipc.engineInstall(r.id).catch((e) => say(r.id, String(e)))}>Update</button>}
              {r.status.state === "installed" && <button onClick={() => verify(r.id)}>Verify</button>}
              {(r.status.state === "installed" || r.status.state === "damaged") && (
                <button className="ghost" onClick={() => ipc.engineRemove(r.id).then(refresh, (e) => say(r.id, String(e)))}>Remove</button>
              )}
            </div>
          </li>
        ))}
        <li className="row muted">
          <span>LibreOffice (Office documents), LaTeX (PDF from text)</span>
          <span>Coming later</span>
        </li>
      </ul>
      <p className="muted">Disk used: {formatBytes(total)}</p>

      <h2>Third-party notices</h2>
      <ul className="rows">
        {notices.map((n) => (
          <li key={n.name} className="row">
            <div>
              <strong>{n.name}</strong> <span className="muted">{n.licence}{n.bundled ? " · included with Recast" : " · downloaded from upstream"}</span>
            </div>
            <div className="job-actions">
              {/* Reveal, not open: licence files are often extensionless (LICENSE, COPYING). */}
              {n.licence_file && <button onClick={() => ipc.reveal(n.licence_file!)}>Licence</button>}
              <button onClick={() => openUrl(n.homepage)}>Website</button>
              {n.source_url && <button onClick={() => openUrl(n.source_url!)}>Source</button>}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

function describe(r: EngineRow): string {
  switch (r.status.state) {
    case "not_installed":
      return "Not installed. Downloads the first time you need it.";
    case "installing":
      return `Installing · ${Math.round(r.status.pct * 100)}%`;
    case "installed":
      return `Installed${r.status.update_available ? ` (${r.status.version}; ${r.version} available)` : ""} · ${formatBytes(r.disk_bytes)}`;
    case "damaged":
      return "Damaged";
  }
}
```

- [ ] **Step 2: Write `src/companion/Settings.tsx`**

```tsx
import { useEffect, useState, type ReactNode } from "react";
import { ipc } from "../shared/ipc";
import type { DeepPartial, Settings } from "../shared/types";

export function SettingsView() {
  const [s, setS] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc.getSettings().then(setS).catch((e) => setError(String(e)));
  }, []);

  const save = (patch: DeepPartial<Settings>) =>
    ipc.setSettings(patch).then(setS).catch((e) => setError(String(e)));

  if (!s) return <p className="muted">{error ?? "Loading…"}</p>;

  return (
    <section aria-label="Settings">
      {error && <p className="error" role="alert">{error}</p>}

      <h2>Output</h2>
      <Field label="Save converted files">
        <select
          value={s.output.location}
          onChange={(e) => save({ output: { location: e.target.value as Settings["output"]["location"] } })}
        >
          <option value="next_to_original">Next to the original</option>
          <option value="folder">In a fixed folder</option>
        </select>
      </Field>
      {s.output.location === "folder" && (
        <Field label="Folder">
          <TextInput value={s.output.folder} placeholder="C:\Users\you\Converted" onCommit={(v) => save({ output: { folder: v } })} />
        </Field>
      )}
      <Field label="Fallback folder">
        <TextInput value={s.output.fallback_folder} placeholder="Documents\Recast" onCommit={(v) => save({ output: { fallback_folder: v } })} />
      </Field>

      <h2>Quality</h2>
      <Field label="MP3 bitrate">
        <Kbps value={s.quality.mp3_kbps} onChange={(v) => save({ quality: { mp3_kbps: v } })} />
      </Field>
      <Field label="AAC / M4A bitrate">
        <Kbps value={s.quality.aac_kbps} onChange={(v) => save({ quality: { aac_kbps: v } })} />
      </Field>
      <Field label="JPG quality (1–100)">
        <NumberInput value={s.quality.jpg_q} min={1} max={100} onCommit={(v) => save({ quality: { jpg_q: v } })} />
      </Field>
      <Field label="WebP quality (1–100)">
        <NumberInput value={s.quality.webp_q} min={1} max={100} onCommit={(v) => save({ quality: { webp_q: v } })} />
      </Field>
      <Field label="AVIF quality (1–100)">
        <NumberInput value={s.quality.avif_q} min={1} max={100} onCommit={(v) => save({ quality: { avif_q: v } })} />
      </Field>
      <Field label="GIF frames per second">
        <NumberInput value={s.gif.fps} min={1} max={50} onCommit={(v) => save({ gif: { fps: v } })} />
      </Field>
      <Field label="GIF width (px)">
        <NumberInput value={s.gif.width} min={16} max={4096} onCommit={(v) => save({ gif: { width: v } })} />
      </Field>
      <Field label="Shrink video to (MB)">
        <NumberInput value={s.shrink.target_mb} min={1} max={100000} onCommit={(v) => save({ shrink: { target_mb: v } })} />
      </Field>
      <Field label="Shrink images to (longest edge, px)">
        <NumberInput value={s.shrink.image_px} min={16} max={16384} onCommit={(v) => save({ shrink: { image_px: v } })} />
      </Field>

      <h2>Jobs</h2>
      <Field label="Run at the same time">
        <select value={s.jobs.concurrency} onChange={(e) => save({ jobs: { concurrency: Number(e.target.value) } })}>
          {[1, 2, 3, 4].map((n) => (
            <option key={n} value={n}>{n}</option>
          ))}
        </select>
      </Field>
      <Field label="Stop a stuck job after (seconds)">
        <NumberInput value={s.jobs.stall_timeout_s} min={10} max={3600} onCommit={(v) => save({ jobs: { stall_timeout_s: v } })} />
      </Field>
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function Kbps({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  return (
    <select value={value} onChange={(e) => onChange(Number(e.target.value))}>
      {[96, 128, 160, 192, 256, 320].map((k) => (
        <option key={k} value={k}>{k} kbps</option>
      ))}
    </select>
  );
}

/** Commits on blur or Enter, so typing doesn't save every keystroke. */
function NumberInput({ value, min, max, onCommit }: { value: number; min: number; max: number; onCommit: (v: number) => void }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(draft));
    if (Number.isFinite(n) && n >= min && n <= max && n !== value) onCommit(n);
    else setDraft(String(value));
  };
  return (
    <input
      type="number"
      min={min}
      max={max}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

function TextInput({ value, placeholder, onCommit }: { value: string; placeholder: string; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => draft !== value && onCommit(draft.trim());
  return (
    <input
      type="text"
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}
```

- [ ] **Step 3: Type-check, test and look at it**

```bash
npm run build && npm test
```
Expected: pass. Then run `npm run tauri dev` (in the background) and take a screenshot of the window (the `run` skill can drive this). Check: sidebar with three sections; Library lists five converters plus 7-Zip under notices; Settings shows current values.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): Library (engines, notices) and Settings views"
```

---

### Task 20: Real-engine integration tests

**Files:**
- Create: `src-tauri/tests/engines_integration.rs`

These tests download every engine once (~250 MB, cached under `src-tauri/target/engine-cache/`) and run **every preset** on generated fixtures with awkward names (spec §9).

- [ ] **Step 1: Write the test file**

```rust
//! Real engines (spec §9 "Rust integration" and "Engines").
//! Run: cargo test --features engines --test engines_integration -- --test-threads=1
#![cfg(feature = "engines")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use recast_lib::engines::install::{self, EngineStatus, Fetcher, HttpFetcher, InstallCtx, LocalFetcher};
use recast_lib::engines::manifest::{Manifest, BUNDLED};
use recast_lib::engines::Engines;
use recast_lib::history::History;
use recast_lib::jobs::{Events, JobEvent, JobManager};
use recast_lib::presets::{Catalog, BUNDLED as PRESETS};
use recast_lib::proc::run_capture;
use recast_lib::settings::Settings;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn sevenzip() -> PathBuf {
    manifest_dir().join("bin").join("7z.exe")
}
fn cache() -> PathBuf {
    manifest_dir().join("target").join("engine-cache")
}

/// Downloads each archive once into the cache, then copies from there.
struct CachedFetcher;
impl Fetcher for CachedFetcher {
    fn fetch(&self, url: &str, dest: &Path, p: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
        let cached = cache().join("downloads").join(url.rsplit('/').next().unwrap());
        if !cached.exists() {
            std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
            let part = cached.with_extension("part");
            HttpFetcher.fetch(url, &part, p)?;
            std::fs::rename(&part, &cached).map_err(|e| e.to_string())?;
        }
        LocalFetcher { source: cached }.fetch(url, dest, p)
    }
}

fn engines() -> Arc<Engines> {
    static E: OnceLock<Arc<Engines>> = OnceLock::new();
    E.get_or_init(|| {
        let e = Arc::new(Engines::new(Manifest::parse(BUNDLED).unwrap(), cache().join("engines"), sevenzip(), Box::new(CachedFetcher)));
        for id in ["ffmpeg", "imagemagick", "pandoc", "qpdf", "ghostscript"] {
            e.ensure(id, &mut |_| {}).unwrap_or_else(|err| panic!("{id}: {err}"));
        }
        e
    })
    .clone()
}

fn bin(engine: &str, b: &str) -> PathBuf {
    engines().bin(engine, b).unwrap()
}

fn tool(engine: &str, b: &str, args: &[&str], cwd: &Path) {
    let out = Command::new(bin(engine, b)).args(args).current_dir(cwd).output().unwrap();
    assert!(out.status.success(), "{b} {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

#[derive(Default)]
struct Collect {
    evs: Mutex<Vec<JobEvent>>,
    cv: Condvar,
}
impl Events for Collect {
    fn emit(&self, ev: &JobEvent) {
        self.evs.lock().unwrap().push(ev.clone());
        self.cv.notify_all();
    }
}
impl Collect {
    fn wait_end(&self, id: &str) -> JobEvent {
        let deadline = Instant::now() + Duration::from_secs(600);
        let mut g = self.evs.lock().unwrap();
        loop {
            if let Some(e) = g.iter().find(|e| e.job_id == id && ["done", "skipped", "failed", "cancelled"].contains(&e.status.as_str())) {
                return e.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "job {id} timed out");
            g = self.cv.wait_timeout(g, left).unwrap().0;
        }
    }
}

struct Rig {
    mgr: JobManager,
    events: Arc<Collect>,
    catalog: Arc<Catalog>,
}

fn rig(f: impl Fn(&mut Settings) + Send + Sync + 'static) -> Rig {
    let catalog = Arc::new(Catalog::parse(PRESETS).unwrap());
    let events = Arc::new(Collect::default());
    let mgr = JobManager::new(
        catalog.clone(),
        engines(),
        Arc::new(Mutex::new(History::open_in_memory().unwrap())),
        Arc::new(move || {
            let mut s = Settings::default();
            f(&mut s);
            s
        }),
        events.clone(),
        cache().join("scratch"),
    );
    Rig { mgr, events, catalog }
}

impl Rig {
    fn run(&self, preset: &str, inputs: &[&Path]) -> JobEvent {
        let paths: Vec<PathBuf> = inputs.iter().map(|p| p.to_path_buf()).collect();
        let id = self.mgr.start_jobs(preset, &paths).unwrap().remove(0);
        self.events.wait_end(&id)
    }
    fn done(&self, preset: &str, inputs: &[&Path]) -> PathBuf {
        let ev = self.run(preset, inputs);
        assert_eq!(ev.status, "done", "{preset}: {:?}", ev.error);
        PathBuf::from(ev.out_path.unwrap())
    }
}

/// Fixtures with Unicode, `%`, `[ ]` and a leading `-` in their names (spec §9).
struct Fixtures {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

const ODD: &str = "café 日本 %[1] -";

impl Fixtures {
    fn new() -> Self {
        let dir = tempfile::Builder::new().prefix("recast café 日本 ").tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let r = &root;
        let ff = |args: &[&str]| tool("ffmpeg", "ffmpeg", &[&["-hide_banner", "-loglevel", "error", "-y"], args].concat(), r);
        let mk = |args: &[&str]| tool("imagemagick", "magick", args, r);
        ff(&["-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
             "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", "v.mp4"]);
        ff(&["-f", "lavfi", "-i", "testsrc2=duration=2:size=320x240:rate=25", "-f", "lavfi", "-i", "sine=duration=2",
             "-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-tag:v", "hvc1", "-c:a", "aac", "-shortest", "h.mov"]);
        ff(&["-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=25", "-c:v", "libx264", "-pix_fmt", "yuv420p", "s.mp4"]);
        ff(&["-f", "lavfi", "-i", "sine=frequency=440:duration=2", "a.wav"]);
        ff(&["-f", "lavfi", "-i", "nullsrc=s=320x180:d=10,geq=random(1)*255:128:128", "-f", "lavfi", "-i", "sine=duration=10",
             "-c:v", "libx264", "-crf", "0", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", "big.mp4"]);
        mk(&["-size", "64x64", "xc:none", "-fill", "red", "-draw", "circle 32,32 32,8", "PNG32:alpha.png"]);
        mk(&["-delay", "10", "-size", "32x32", "xc:red", "xc:blue", "-loop", "0", "anim.gif"]);
        mk(&["-size", "200x200", "xc:white", "xc:gray", "xc:black", "three.pdf"]);
        mk(&["-size", "20x20", "xc:blue", "pic.png"]);
        if engines().magick_caps().unwrap().contains("magick:write:heic") {
            mk(&["rose:", "photo.heic"]);
        }
        std::fs::write(r.join("book.md"), "# Title\n\nHello ![pic](pic.png)\n").unwrap();
        std::fs::write(r.join("note.txt"), "Just *text*.\n").unwrap();
        for f in ["v.mp4", "h.mov", "s.mp4", "a.wav", "big.mp4", "alpha.png", "anim.gif", "three.pdf", "photo.heic", "note.txt"] {
            if r.join(f).exists() {
                std::fs::rename(r.join(f), r.join(format!("{ODD}{f}"))).unwrap();
            }
        }
        Self { _dir: dir, root }
    }
    fn p(&self, name: &str) -> PathBuf {
        let odd = self.root.join(format!("{ODD}{name}"));
        if odd.exists() { odd } else { self.root.join(name) }
    }
    fn leftovers(&self) -> Vec<PathBuf> {
        recast_lib::fsutil::walk_files(&self.root)
            .into_iter()
            .chain(std::fs::read_dir(&self.root).unwrap().flatten().map(|e| e.path()))
            .filter(|p| p.to_string_lossy().contains(".recast-"))
            .collect()
    }
}

fn ffprobe(p: &Path) -> serde_json::Value {
    let out = run_capture(&bin("ffmpeg", "ffprobe"), [std::ffi::OsStr::new("-v"), "error".as_ref(), "-print_format".as_ref(), "json".as_ref(),
        "-show_streams".as_ref(), "-show_format".as_ref(), p.as_os_str()], Duration::from_secs(30)).expect("ffprobe");
    serde_json::from_str(&out).unwrap()
}

fn first_codec(p: &Path, kind: &str) -> String {
    ffprobe(p)["streams"].as_array().unwrap().iter().find(|s| s["codec_type"] == kind).unwrap()["codec_name"].as_str().unwrap().to_string()
}

/// `magick identify` on a staged copy: output names contain `%[1]`, which ImageMagick
/// would parse as a property escape (the same reason presets stage their inputs).
fn identify(p: &Path, fmt: &str) -> String {
    let d = tempfile::tempdir().unwrap();
    let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let staged = format!("x.{ext}");
    std::fs::copy(p, d.path().join(&staged)).unwrap();
    let out = Command::new(bin("imagemagick", "magick"))
        .args(["identify", "-format", fmt, &format!("{staged}[0]")])
        .current_dir(d.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "identify {}: {}", p.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn qpdf_check(p: &Path) {
    let out = Command::new(bin("qpdf", "qpdf")).arg("--check").arg(p).output().unwrap();
    // 0 = clean, 3 = warnings only
    assert!(matches!(out.status.code(), Some(0) | Some(3)), "qpdf --check {}: {}", p.display(), String::from_utf8_lossy(&out.stdout));
}

fn npages(p: &Path) -> u32 {
    run_capture(&bin("qpdf", "qpdf"), [std::ffi::OsStr::new("--show-npages"), p.as_os_str()], Duration::from_secs(30)).unwrap().trim().parse().unwrap()
}

#[test]
fn inspect_uses_ffprobe_to_hide_audio_presets_for_silent_video() {
    let fx = Fixtures::new();
    let catalog = Catalog::parse(PRESETS).unwrap();
    let ids = |name: &str| -> Vec<String> {
        let m = recast_lib::pad_model::build(&catalog, &engines(), &[fx.p(name).to_string_lossy().into_owned()]).unwrap();
        m.presets.into_iter().map(|p| p.id).collect()
    };
    let silent = ids("s.mp4");
    for gone in ["video-mp3", "video-m4a", "video-opus", "video-wav"] {
        assert!(!silent.contains(&gone.to_string()), "{gone} shown for a silent video");
    }
    assert!(ids("v.mp4").contains(&"video-mp3".to_string()));
}

#[test]
fn video_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let v = fx.p("v.mp4");
    for (preset, codec_kind, codec) in [
        ("video-mp3", "audio", "mp3"), ("video-gif", "video", "gif"), ("video-webm", "video", "vp9"),
        ("video-m4a", "audio", "aac"), ("video-mkv", "video", "h264"), ("video-mov", "video", "h264"),
        ("video-opus", "audio", "opus"), ("video-wav", "audio", "pcm_s16le"),
    ] {
        assert_eq!(first_codec(&r.done(preset, &[&v]), codec_kind), codec, "{preset}");
    }
    // 10-bit HEVC iPhone-style source → 8-bit yuv420p H.264 (spec §5.1 MP4)
    let mp4 = r.done("video-mp4", &[&fx.p("h.mov")]);
    let s = &ffprobe(&mp4)["streams"][0];
    assert_eq!((s["codec_name"].as_str(), s["pix_fmt"].as_str()), (Some("h264"), Some("yuv420p")));
    assert!(fx.leftovers().is_empty());
}

#[test]
fn shrink_hits_the_target_and_parallel_jobs_dont_collide() {
    let fx = Fixtures::new();
    let r = rig(|s| s.shrink.target_mb = 1);
    assert_eq!(r.run("video-shrink", &[&fx.p("v.mp4")]).status, "skipped", "already under 1 MB");
    let big = fx.p("big.mp4");
    let copy = fx.root.join("big copy.mp4");
    std::fs::copy(&big, &copy).unwrap();
    let ids = [
        r.mgr.start_jobs("video-shrink", &[big.clone()]).unwrap().remove(0),
        r.mgr.start_jobs("video-shrink", &[copy.clone()]).unwrap().remove(0),
    ];
    for id in ids {
        let ev = r.events.wait_end(&id);
        assert_eq!(ev.status, "done", "{:?}", ev.error);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert!(std::fs::metadata(&out).unwrap().len() <= 1024 * 1024, "{} too big", out.display());
    }
    assert!(fx.leftovers().is_empty());
}

#[test]
fn audio_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let a = fx.p("a.wav");
    for (preset, codec) in [
        ("audio-mp3", "mp3"), ("audio-m4a", "aac"), ("audio-opus", "opus"), ("audio-flac", "flac"),
        ("audio-alac", "alac"), ("audio-aiff", "pcm_s16be"), ("audio-ogg", "vorbis"),
    ] {
        assert_eq!(first_codec(&r.done(preset, &[&a]), "audio"), codec, "{preset}");
    }
}

#[test]
fn image_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let png = fx.p("alpha.png");
    let has_alpha = |p: &Path| !matches!(identify(p, "%A").as_str(), "Undefined" | "False");
    for (preset, magick_fmt, alpha) in [
        ("image-jpg", "JPEG", Some(false)), ("image-webp", "WEBP", Some(true)), ("image-avif", "AVIF", None),
        ("image-jxl", "JXL", None), ("image-ico", "ICO", Some(true)), ("image-tiff", "TIFF", None), ("image-pdf", "PDF", None),
    ] {
        let out = r.done(preset, &[&png]);
        if magick_fmt != "PDF" {
            assert_eq!(identify(&out, "%m"), magick_fmt, "{preset}");
        }
        if let Some(a) = alpha {
            assert_eq!(has_alpha(&out), a, "{preset} alpha");
        }
    }
    let shrunk = r.done("image-shrink", &[&png]);
    assert!(shrunk.file_name().unwrap().to_string_lossy().ends_with("(compressed).png"));
    // animated GIF → exactly one PNG
    let one = r.done("image-png", &[&fx.p("anim.gif")]);
    assert!(one.to_string_lossy().ends_with("anim.png"));
    assert!(!fx.root.join(format!("{ODD}anim-1.png")).exists());
    // HEIC read (if the build writes HEIC we made a fixture)
    if fx.p("photo.heic").exists() {
        r.done("image-jpg", &[&fx.p("photo.heic")]);
    }
    let combined = r.done("image-combine-pdf", &[&png, &fx.p("pic.png")]);
    assert_eq!(npages(&combined), 2);
    assert!(fx.leftovers().is_empty());
}

#[test]
fn pdf_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let pdf = fx.p("three.pdf");
    let c = r.run("pdf-compress", &[&pdf]);
    assert!(c.status == "done" || c.status == "skipped", "{:?}", c.error);
    let pages = r.done("pdf-images", &[&pdf]);
    assert_eq!(std::fs::read_dir(&pages).unwrap().count(), 3);
    let split = r.done("pdf-split", &[&pdf]);
    assert_eq!(std::fs::read_dir(&split).unwrap().count(), 3);
    let merged = r.done("pdf-merge", &[&pdf, &pdf]);
    assert_eq!(merged.file_name().unwrap(), "merged.pdf");
    assert_eq!(npages(&merged), 6);
    qpdf_check(&merged);
    for page in std::fs::read_dir(&split).unwrap().flatten() {
        qpdf_check(&page.path());
    }
    if c.status == "done" {
        qpdf_check(Path::new(c.out_path.as_deref().unwrap()));
    }
    assert!(fx.leftovers().is_empty());
}

#[test]
fn text_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let md = fx.p("book.md");
    for preset in ["text-epub", "text-docx", "text-odt"] {
        assert!(r.done(preset, &[&md]).is_file(), "{preset}");
    }
    // EPUB as input
    let epub = fx.root.join("book.epub");
    assert!(r.done("text-docx", &[&epub]).is_file());
    let html = r.done("text-html", &[&md]);
    assert!(std::fs::read_to_string(&html).unwrap().contains("data:image"), "image embedded");
    let docx = fx.root.join("book.docx");
    let back = r.done("text-md", &[&docx]);
    let media = back.with_file_name(format!("{}_media", back.file_stem().unwrap().to_string_lossy()));
    assert!(media.is_dir(), "media folder beside {}", back.display());
    assert!(r.done("text-md", &[&fx.p("note.txt")]).is_file());
    assert!(fx.leftovers().is_empty());
}

#[test]
fn paths_over_260_characters() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let mut deep = fx.root.clone();
    while deep.as_os_str().len() < 280 {
        deep.push("a-long-folder-name-to-pass-max-path");
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::copy(fx.p("pic.png"), deep.join("pic.png")).unwrap(); // book.md embeds it
    for (name, preset) in [("v.mp4", "video-mp3"), ("alpha.png", "image-png"), ("three.pdf", "pdf-split"), ("three.pdf", "pdf-compress"), ("book.md", "text-html")] {
        let src = deep.join(name);
        if !src.exists() {
            std::fs::copy(fx.p(name), &src).unwrap();
        }
        let ev = r.run(preset, &[&src]);
        assert!(ev.status == "done" || ev.status == "skipped", "{preset} on a long path: {:?}", ev.error);
    }
}

#[test]
fn install_rejects_corruption_and_verify_catches_tampering() {
    let e = engines(); // ensures the download cache is filled
    let manifest = Manifest::parse(BUNDLED).unwrap();
    let qpdf = manifest.get("qpdf").unwrap();
    let cached = cache().join("downloads").join(qpdf.url.rsplit('/').next().unwrap());
    let d = tempfile::tempdir().unwrap();
    let bad = d.path().join("bad.zip");
    let mut bytes = std::fs::read(&cached).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&bad, bytes).unwrap();
    let root = d.path().join("engines");
    let fetcher = LocalFetcher { source: bad };
    let err = install::install(qpdf, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap_err();
    assert!(err.contains("checksum"));
    assert_eq!(install::read_status(qpdf, &root), EngineStatus::NotInstalled);

    // Ghostscript unpacked from its NSIS installer, never run as an installer, works from its folder
    let gs = e.bin("ghostscript", "gswin64c").unwrap();
    assert!(run_capture(&gs, ["-v"], Duration::from_secs(30)).unwrap().contains("Ghostscript"));

    // Verify catches a modified exe in a private copy
    let fetcher = LocalFetcher { source: cached };
    install::install(qpdf, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap();
    assert!(install::verify(qpdf, &root).unwrap());
    let (dir, rec) = install::installed(qpdf, &root).unwrap();
    let exe = dir.join(&rec.bins["qpdf"].rel);
    let mut b = std::fs::read(&exe).unwrap();
    b.push(0);
    std::fs::write(&exe, b).unwrap();
    assert!(!install::verify(qpdf, &root).unwrap());
}
```

> `"error".as_ref()` inside an array with an `&OsStr` first element infers `&OsStr`; if your compiler asks for annotations, write `OsStr::new("error")` for each.

- [ ] **Step 2: Run them**

```bash
cd src-tauri && cargo test --features engines --test engines_integration -- --test-threads=1
```
Expected: 9 passed (first run downloads ~250 MB). **When one fails, read the engine's stderr in the assertion message and fix the cause** (a preset's args in `presets.toml`, a missing capability, a naming bug). Do not loosen an assertion unless the spec itself is wrong. The one sanctioned exception: if an engine (not Recast) cannot handle a >260-character path even with staging, change that single row in `paths_over_260_characters` to expect `failed` with a comment naming the engine, and record the limitation in the spec's §11.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "test: real-engine integration tests for every preset, odd names and long paths"
```

---

### Task 21: Final verification, installer build and docs

**Files:**
- Modify: `README.md`, `docs/superpowers/specs/2026-11-18-recast-design.md` (fold in the deviations listed at the top of this plan)

- [ ] **Step 1: Full suite**

```bash
npm test && npm run build
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
cd src-tauri && cargo test --features engines --test engines_integration -- --test-threads=1
```
Expected: everything passes.

- [ ] **Step 2: Build the installer and check its size (spec goal 4: ~15 MB)**

```bash
npm run tauri build
ls -la src-tauri/target/release/bundle/nsis/
```
Expected: `Recast_0.1.0_x64-setup.exe` exists. Record its size in the README. If it's over 20 MB, find out why (`7z.dll` is ~1.8 MB; the app itself should be under 15 MB) before going on.

- [ ] **Step 3: Manual end-to-end pass in the real app** (`npm run tauri dev`)

Check each and note results in the final report:
1. Drop a `.mov` on Activity → presets listed with "Downloads FFmpeg" badges → pick MP3 → "Getting the converter n%" → progress → done → notification "x.mp3 ready" → Show in folder works.
2. Drop 3 PNGs → JPG → one combined notification "3 files converted".
3. Drop a PDF and a PNG together → "These files are different kinds…".
4. Start a long conversion and Cancel → row says Cancelled; no `.recast-*` folder next to the source.
5. Library: engines show Installed with size; Verify says "Verified"; Remove then re-Download works; Third-party notices lists all six with working Licence/Website/Source buttons.
6. Settings: change MP3 bitrate to 320, convert again, `ffprobe` shows 320 kb/s; restart the app and the value persists.
7. Close the window → app stays in the tray; tray left-click reopens; tray Quit exits.
8. Quit mid-conversion, relaunch → the job shows "Interrupted when Recast closed" and the `.recast-*` folder is gone.

- [ ] **Step 4: Fold the deviations into the spec**

Edit the spec: §6.3 (7-Zip full, `bins` as file names, `check`, `capabilities`), §5 (preset fields list, `fixed:` naming), §6.4 (cwd = temp dir, relative `{out}`, staged inputs, scratch temp root for long destinations), §6.13 (repo layout as built). Update the README with the installer size.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs: fold Plan 1 implementation decisions into the spec"
```

---

## After Plan 1

Write **Plan 2 (drop pad)** and **Plan 3 (triggers, shell, installer hooks)** from the spec once this plan has landed, using the real IPC and types built here.
