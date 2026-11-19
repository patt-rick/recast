# Recast Plan 3: Triggers, Shell, Notifications, First Run and Installer Hooks

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the drop pad reachable the ways the spec intends. That means the edge handle, the desk pad, shaking a file mid-drag, and "Open with Recast" in Explorer with single-instance forwarding. Around those it adds start with Windows, notification buttons, tray glyphs, the first-run welcome, the trigger settings and the uninstall hook. It also replaces the mixed-kinds dead end with **Open in Recast**, which opens Activity with the files grouped by kind.

**Architecture:** Every decision is a pure, unit-tested function:
- `shell.rs`: argv parsing, registry entries and the forwarding debounce.
- `triggers/shake.rs`: the shake detector.
- `triggers/geometry.rs`: the handle and desk-pad placement, plus the fullscreen predicate.
- `toast.rs`: notification targets and actions.
- `companion/groups.ts`: grouping files by kind.

The OS calls live only in `platform/win.rs`: HKCU registry writes, the `WH_MOUSE_LL` hook thread, the foreground watcher and the taskbar theme. The two new trigger windows (`handle`, `deskpad`) are tiny Tauri windows on one new Vite entry. Their drag-drop events are handled in Rust, where they call the existing `pad::open` (now `open_at`, with an anchor). A shake opens an *invisible probe* pad that only appears if a real OLE drag enters it within 300 ms.

**Tech Stack:** Tauri 2.12 (`WindowEvent::DragDrop`, `focusable`, `alwaysOnBottom`, `tauri::image::Image` with `image-png`), `tauri-plugin-notification` 2.8.1 (desktop action buttons via `register_action_types` and `on_action`), `tauri-plugin-single-instance` 2.5.5 (argv forwarding; it already calls `AllowSetForegroundWindow` for the primary), Rust plus `windows` 0.62 (registry, `SetWindowsHookExW`, `SetWinEventHook`, monitors), React 19 + TypeScript, Vitest, NSIS hooks.

**Spec:** `docs/superpowers/specs/2026-11-18-recast-design.md`. Section references (§) point there. Plan 2's binding decisions are at the top of `docs/superpowers/plans/2026-11-18-recast-plan-2-pad.md`.

---

## Scope of this plan

Plan 3 delivers:
- §4.1: the edge handle, desk pad, shake and "Open with Recast".
- §4.5 Settings: Triggers, Start with Windows and Explorer entry.
- §4.6: the first run.
- §4.3 and §5.7: notification buttons (Show in folder, Details, Save to fallback folder).
- §7: engine-install Retry.
- §5.6: Open in Recast for mixed kinds.
- §6.7: triggers.
- §6.10: shell and single instance.
- §6.12: the uninstall hook.
- §9: `docs/qa-checklist.md`.
- §12: the tray glyphs.

It removes Plan 2's stand-ins: the tray **Drop pad** item and the debug-only `debug_open_pad` command. **Convert files…** stays.

**Not in Plan 3:** everything in §11, and fixes for Plan 2's unfixed review findings unless a task below touches that code. The handoff's list stays in the handoff doc.

## Deliberate deviations from the spec (fold into the spec in Task 12)

1. **Shake confirmation is the DragEnter alone.** The spec allows "a drag cursor or a `DragEnter` within 300 ms", but OLE drag cursors come from the drag source and can't be told apart reliably through `GetCursorInfo`.
   - Recast confirms a shake only by a `DragEnter` that carries file paths and reaches the pad within 300 ms.
   - When armed, the pad is shown as an **invisible probe**: shown no-activate, nothing drawn and no backdrop captured. A probe that nothing enters within 300 ms is hidden again.
   - A non-OLE press-drag (text selection, painting) keeps mouse capture in its own window, so the probe never sees it.
2. **`AllowSetForegroundWindow` needs no custom hook.** `tauri-plugin-single-instance` 2.5.5 already calls it with the primary's pid before forwarding (see its `platform_impl/windows.rs`).
3. **The edge-handle pad sits beside the handle**, not centred on the cursor, so the handle stays reachable for a drop. Placement is `pad_anchor_beside`, 12 px from the handle. The handle lives on the **primary monitor's work area**.
4. **The desk pad uses a translucent look without blur**: a white tint, a rim and the app mark. Real glass would need a backdrop capture for a window that sits under other windows. A drop or drag-enter on it opens the pad over it. It has no click action.
5. **The fullscreen auto-hide** applies when the foreground window covers the **primary** monitor, because the handle lives there. Shell windows (`Progman`, `WorkerW`, `Shell_TrayWnd`, `Shell_SecondaryTrayWnd`) never count as fullscreen. Shake is off while *any* monitor's foreground window is fullscreen.
6. **Dev builds don't write to the registry.** The Explorer verb and the Run value are written only in release builds, or in a dev build with `RECAST_DEV_INTEGRATION=1`, so a `cargo run` doesn't point Explorer at `target\debug`.
7. **Start with Windows** writes `HKCU\…\Run\Recast = "<exe>" --autostart`. An `--autostart` launch stays in the tray: the companion is created hidden (`visible: false`) and shown at startup only by a plain launch.
8. **Notification buttons only work while the toast is on screen.** The plugin reports clicks only then, not from the Action Center. A click on the toast body (`tap`) opens Activity, or Library for an engine failure.
9. **Mixed kinds:**
   - **In the pad**, the ring shows one segment, **Open**, labelled "Open in Recast · convert each kind on its own" in its pill. It opens the companion's Activity with the files listed in kind groups, each with a **Choose conversion** button.
   - **In the companion**, a mixed drop shows the groups directly, with no ring.
10. **The desk-pad position** is stored as `triggers.desk_x` / `triggers.desk_y` (physical px, `-1` = not set, which means the bottom-right of the primary work area).
11. **The shake detector is tested with synthetic sample streams** (straight drags, jitter, slow and fast shakes, vertical shakes) rather than recorded ones. `docs/qa-checklist.md` covers real-world false positives.

## Review amendments (Opus plan review, binding: these override the task text below)

_Filled in after the Opus plan review._

## File structure

```
recast/
  widgets.html                      # NEW third Vite entry (handle + desk pad)
  vite.config.ts                    # MODIFY: widgets entry
  src/
    widgets/main.tsx, Widget.tsx, widgets.css      # NEW
    pad/openInRecast.ts (+test)     # NEW pseudo preset for mixed kinds
    pad/PadApp.tsx                  # MODIFY: probe opens, Open in Recast
    pad/usePad.ts                   # MODIFY: Open in Recast runs ipc.openInRecast
    companion/groups.ts (+test)     # NEW files → kind groups
    companion/Activity.tsx          # MODIFY: groups panel, incoming files, focus a job
    companion/InlineRing.tsx        # MODIFY: onStarted
    companion/App.tsx               # MODIFY: settings lifted, navigate/open-files events, Welcome
    companion/Settings.tsx          # MODIFY: Triggers + Windows sections, props instead of own state
    companion/Welcome.tsx           # NEW first-run welcome
    companion/companion.css         # MODIFY
    shared/types.ts, ipc.ts         # MODIFY
  src-tauri/
    Cargo.toml                      # MODIFY: windows features, tauri image-png
    tauri.conf.json                 # MODIFY: companion hidden, handle + deskpad windows, NSIS hooks
    build.rs                        # MODIFY: commands
    capabilities/default.json, pad.json   # MODIFY
    capabilities/widgets.json       # NEW
    windows/hooks.nsh               # NEW NSIS_HOOK_POSTUNINSTALL
    icons/tray-dark-taskbar.png, tray-light-taskbar.png   # NEW (rendered from assets/icon/*.svg)
    src/
      shell.rs                      # NEW argv, registry entries, forwarding debounce
      integration.rs                # NEW applies settings to the registry and triggers
      toast.rs                      # NEW notifications with buttons
      triggers/mod.rs               # NEW windows, shake hook, foreground tick
      triggers/geometry.rs          # NEW handle/desk placement, fullscreen predicate
      triggers/shake.rs             # NEW shake detector
      pad.rs                        # MODIFY: open_at, probe, mode tracking, release grace
      platform/win.rs               # MODIFY: registry, theme, foreground, mouse hook
      jobs/notify.rs                # MODIFY: notification targets
      app_state.rs                  # MODIFY: update_settings, engine toasts
      ipc.rs                        # MODIFY: new commands, set_settings via update_settings
      settings.rs                   # MODIFY: desk_x, desk_y
      tray.rs                       # MODIFY: glyphs, no Drop pad
      lib.rs                        # MODIFY: wiring
  docs/qa-checklist.md              # NEW
```

**Conventions for every task**
- Rust commands run from `src-tauri/`, npm commands from the repo root. Use the Bash tool (Git Bash).
- **The machine is low on memory.** Run one cargo build at a time with `-j 2`, and never run builds in parallel agents.
- **Rust test filters go after `--`** (`cargo test -j 2 -- shell::`). A filter before `--` skips building `examples/fake_engine.rs`, and the runner and job tests then fail with "fake_engine.exe is missing".
- **Write files with the Write/Edit tools, not shell heredocs.** Heredocs have collapsed `\\` to `\` before. Re-read backslash-heavy strings (registry paths, Windows paths) after writing them.
- **`windows` 0.62 signatures:** the code below was written against the crate's generated signatures (checked for `SetWindowsHookExW`, `CallNextHookEx`, `GetMessageW`, `PostThreadMessageW`, `SetTimer`, `SetWinEventHook`, `MonitorFromWindow`, `GetMonitorInfoW` and `RegSetKeyValueW`/`RegGetValueW`/`RegDeleteTreeW`/`RegDeleteKeyValueW`). If a call doesn't compile, fix the call against `~/.cargo/registry/src/*/windows-0.62.*/src/Windows/Win32/...`. Don't change the behaviour.
- A running `recast.exe` locks `target/debug/recast.exe`. Either stop only the process you started, or build with `CARGO_TARGET_DIR="$LOCALAPPDATA/Temp/recast-p3"`.
- **Never write to the real registry from tests except under `HKCU\Software\RecastTest-<uuid>`**, and delete that key at the end of the test.
- Commit after each task with the message given. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Front-end checks: `npm test` (Vitest) and `npm run build` (runs `tsc --noEmit`). Rust checks: `cargo test -j 2`, `cargo clippy -j 2 --all-targets -- -D warnings` and `cargo fmt --check`.

**Execution groups and review checkpoints**

| Group | Tasks | Then |
|---|---|---|
| A: shell and settings | 1–3 | |
| B: pad and triggers | 4–7 | **Opus checkpoint review 1** |
| C: notifications, tray, companion | 8–11 | **Opus checkpoint review 2** |
| D: wrap-up | 12 | **Final Opus review** |

---

### Task 1: Registry and theme helpers in `platform`

**Files:**
- Modify: `src-tauri/src/platform/win.rs`
- Modify: `src-tauri/Cargo.toml` (no new `windows` features needed yet: Registry is already enabled)

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` in `win.rs`)

```rust
    fn scratch_key() -> String {
        format!(r"Software\RecastTest-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn registry_strings_round_trip_and_trees_delete() {
        let key = scratch_key();
        let sub = format!(r"{key}\command");
        hkcu_set_string(&key, None, "Open with Recast").unwrap();
        hkcu_set_string(&key, Some("MultiSelectModel"), "Player").unwrap();
        hkcu_set_string(&sub, None, r#""C:\x\recast.exe" --pad "%1""#).unwrap();
        assert_eq!(hkcu_get_string(&key, None).as_deref(), Some("Open with Recast"));
        assert_eq!(hkcu_get_string(&key, Some("MultiSelectModel")).as_deref(), Some("Player"));
        assert_eq!(
            hkcu_get_string(&sub, None).as_deref(),
            Some(r#""C:\x\recast.exe" --pad "%1""#)
        );
        hkcu_delete_value(&key, "MultiSelectModel").unwrap();
        assert_eq!(hkcu_get_string(&key, Some("MultiSelectModel")), None);
        hkcu_delete_value(&key, "MultiSelectModel").unwrap(); // missing is fine
        hkcu_delete_tree(&key).unwrap();
        assert_eq!(hkcu_get_string(&key, None), None);
        assert_eq!(hkcu_get_string(&sub, None), None);
        hkcu_delete_tree(&key).unwrap(); // missing is fine
    }

    #[test]
    fn dword_reads_and_theme_reads_without_panicking() {
        let key = scratch_key();
        assert_eq!(hkcu_dword(&key, "Nope"), None);
        let _ = taskbar_is_light();
        let _ = transparency_enabled();
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd src-tauri && cargo test -j 2 -- platform::`
Expected: compile errors, because `hkcu_set_string` and the other helpers are not found.

- [ ] **Step 3: Implement** (add to `win.rs`, and extend the `Registry` import)

```rust
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW,
    RegGetValueW, RegSetKeyValueW,
};

fn wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_result(e: WIN32_ERROR) -> io::Result<()> {
    if e.is_ok() {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(e.0 as i32))
    }
}

/// Write a REG_SZ value under HKCU, creating the key. `name` None is the key's default value.
pub fn hkcu_set_string(key: &str, name: Option<&str>, data: &str) -> io::Result<()> {
    let (k, n, d) = (wstr(key), name.map(wstr), wstr(data));
    let n = n.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            n,
            REG_SZ.0,
            Some(d.as_ptr() as *const c_void),
            (d.len() * 2) as u32,
        )
    };
    reg_result(r)
}

/// A REG_SZ value under HKCU, or None if the key or value is missing.
pub fn hkcu_get_string(key: &str, name: Option<&str>) -> Option<String> {
    let (k, n) = (wstr(key), name.map(wstr));
    let n = n.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let mut size = 0u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), n, RRF_RT_REG_SZ, None, None, Some(&mut size))
    };
    if r.is_err() || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            n,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Delete an HKCU key with everything under it. A missing key is not an error.
pub fn hkcu_delete_tree(key: &str) -> io::Result<()> {
    let k = wstr(key);
    let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr())) };
    if r == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    reg_result(r)
}

/// Delete one HKCU value. A missing key or value is not an error.
pub fn hkcu_delete_value(key: &str, name: &str) -> io::Result<()> {
    let (k, n) = (wstr(key), wstr(name));
    let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr())) };
    if r == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    reg_result(r)
}

/// A REG_DWORD value under HKCU, or None if missing.
pub fn hkcu_dword(key: &str, name: &str) -> Option<u32> {
    let (k, n) = (wstr(key), wstr(name));
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            PCWSTR(n.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    r.is_ok().then_some(value)
}

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// The taskbar (and tray) uses the light theme. A missing value means dark, as Windows 10 defaults.
pub fn taskbar_is_light() -> bool {
    hkcu_dword(PERSONALIZE, "SystemUsesLightTheme") == Some(1)
}
```

Replace the body of `transparency_enabled` with `hkcu_dword(PERSONALIZE, "EnableTransparency") != Some(0)`. Delete its old `RegGetValueW` block and the now-unused `w!` import if nothing else uses it. Keep the doc comment.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test -j 2 -- platform::`
Expected: PASS, including the two new tests.

- [ ] **Step 5: Lint and commit**

```bash
cd src-tauri && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt
git add src-tauri/src/platform/win.rs
git commit -m "feat(platform): HKCU registry helpers and the taskbar theme"
```

---

### Task 2: `shell.rs`: argv, registry entries, forwarding debounce (pure)

**Files:**
- Create: `src-tauri/src/shell.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod shell;`)

- [ ] **Step 1: Write the module with its tests first**

Write `src-tauri/src/shell.rs` with only the `#[cfg(test)] mod tests` block below plus empty `pub fn` signatures returning `todo!()`. Then run the tests and see them fail with the panic. Then fill in the bodies as in Step 3.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_pad_paths_and_autostart() {
        let cwd = Path::new(r"C:\work");
        assert_eq!(parse_args(&argv(&["recast.exe"]), cwd), Launch::default());
        assert_eq!(
            parse_args(&argv(&["recast.exe", "--autostart"]), cwd),
            Launch { pad: vec![], autostart: true }
        );
        let l = parse_args(&argv(&["recast.exe", "--pad", r"C:\v\a.mov", "b.png"]), cwd);
        assert_eq!(l.pad, vec![r"C:\v\a.mov".to_string(), r"C:\work\b.png".to_string()]);
        assert!(!l.autostart);
        // A later flag ends the path list; empty arguments are skipped.
        let l = parse_args(&argv(&["recast.exe", "--pad", "", r"C:\a.png", "--autostart", r"C:\b.png"]), cwd);
        assert_eq!(l.pad, vec![r"C:\a.png".to_string()]);
        assert!(l.autostart);
    }

    #[test]
    fn verb_entries_match_spec_6_10() {
        let e = verb_entries(r"C:\Users\x\AppData\Local\Recast\recast.exe");
        let get = |key: &str, name: Option<&str>| {
            e.iter().find(|r| r.key == key && r.name == name).map(|r| r.data.clone())
        };
        assert_eq!(get(VERB_KEY, None).as_deref(), Some("Open with Recast"));
        assert_eq!(get(VERB_KEY, Some("MultiSelectModel")).as_deref(), Some("Player"));
        assert_eq!(
            get(VERB_KEY, Some("Icon")).as_deref(),
            Some(r#""C:\Users\x\AppData\Local\Recast\recast.exe",0"#)
        );
        assert_eq!(
            get(&format!(r"{VERB_KEY}\command"), None).as_deref(),
            Some(r#""C:\Users\x\AppData\Local\Recast\recast.exe" --pad "%1""#)
        );
        assert_eq!(VERB_KEY, r"Software\Classes\*\shell\Recast");
        assert_eq!(run_command(r"C:\r\recast.exe"), r#""C:\r\recast.exe" --autostart"#);
    }

    #[test]
    fn collector_slides_300ms_and_caps_at_2s() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut c = Collector::default();
        assert_eq!(c.deadline(), None);
        c.push(vec!["a".into()], ms(0));
        assert_eq!(c.deadline(), Some(ms(300)));
        c.push(vec!["b".into(), "a".into()], ms(200)); // slides; duplicate dropped
        assert_eq!(c.deadline(), Some(ms(500)));
        assert_eq!(c.take_if_due(ms(499)), None);
        assert_eq!(c.take_if_due(ms(500)), Some(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(c.deadline(), None, "taking resets");
        // A steady trickle is flushed 2 s after the first arrival.
        for i in 0..20u64 {
            c.push(vec![format!("f{i}")], ms(1000 + i * 150));
        }
        assert_eq!(c.deadline(), Some(ms(3000)));
        assert_eq!(c.take_if_due(ms(3000)).map(|v| v.len()), Some(14));
        c.push(vec![], ms(4000));
        assert_eq!(c.deadline(), None, "an empty push starts nothing");
    }
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd src-tauri && cargo test -j 2 -- shell::`
Expected: FAIL (`not yet implemented`).

- [ ] **Step 3: Implement**

```rust
//! "Open with Recast" (spec §6.10) and start with Windows: the registry entries, the
//! command line, and the debounce that turns one-process-per-file into one pad.
use std::path::Path;
use std::time::{Duration, Instant};

pub const VERB_KEY: &str = r"Software\Classes\*\shell\Recast";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Same name the Tauri NSIS uninstaller already removes (`${PRODUCTNAME}`).
pub const RUN_VALUE: &str = "Recast";

/// One REG_SZ value to write under HKCU. `name` None is the key's default value.
#[derive(Debug, Clone, PartialEq)]
pub struct RegString {
    pub key: String,
    pub name: Option<&'static str>,
    pub data: String,
}

/// The classic-menu verb. `MultiSelectModel=Player` raises Explorer's limit for a
/// command-line verb from 15 selected items to 100.
pub fn verb_entries(exe: &str) -> Vec<RegString> {
    let s = |key: String, name: Option<&'static str>, data: String| RegString { key, name, data };
    vec![
        s(VERB_KEY.into(), None, "Open with Recast".into()),
        s(VERB_KEY.into(), Some("MultiSelectModel"), "Player".into()),
        s(VERB_KEY.into(), Some("Icon"), format!("\"{exe}\",0")),
        s(format!(r"{VERB_KEY}\command"), None, format!("\"{exe}\" --pad \"%1\"")),
    ]
}

/// The Run value: start in the tray, without opening the companion.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" --autostart")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Launch {
    /// Files to open the pad with (`--pad <paths…>`), made absolute against the cwd.
    pub pad: Vec<String>,
    /// Started by Windows at sign-in: stay in the tray.
    pub autostart: bool,
}

/// `argv[0]` is the exe. `--pad` takes every following argument up to the next `--flag`.
pub fn parse_args(argv: &[String], cwd: &Path) -> Launch {
    let mut l = Launch::default();
    let mut in_pad = false;
    for a in argv.iter().skip(1) {
        match a.as_str() {
            "--autostart" => {
                l.autostart = true;
                in_pad = false;
            }
            "--pad" => in_pad = true,
            s if s.starts_with("--") => in_pad = false,
            "" => {}
            s if in_pad => {
                let p = Path::new(s);
                let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
                l.pad.push(abs.to_string_lossy().into_owned());
            }
            _ => {}
        }
    }
    l
}

/// Explorer starts one process per selected file; their paths arrive one by one.
pub const QUIET: Duration = Duration::from_millis(300);
pub const CAP: Duration = Duration::from_secs(2);

/// Collects forwarded paths until 300 ms pass without a new arrival, or 2 s after the
/// first (spec §6.10).
#[derive(Default, Debug)]
pub struct Collector {
    first: Option<Instant>,
    last: Option<Instant>,
    paths: Vec<String>,
}

impl Collector {
    pub fn push(&mut self, paths: Vec<String>, now: Instant) {
        if paths.is_empty() {
            return;
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
        for p in paths {
            if !self.paths.contains(&p) {
                self.paths.push(p);
            }
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        Some((self.last? + QUIET).min(self.first? + CAP))
    }

    pub fn take_if_due(&mut self, now: Instant) -> Option<Vec<String>> {
        if now < self.deadline()? {
            return None;
        }
        self.first = None;
        self.last = None;
        Some(std::mem::take(&mut self.paths))
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test -j 2 -- shell::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/shell.rs src-tauri/src/lib.rs
git commit -m "feat(shell): argv parsing, Explorer verb and Run entries, forwarding debounce"
```

---

### Task 3: Settings fields, `update_settings`, registry sync and the launch flow

**Files:**
- Modify: `src-tauri/src/settings.rs`, `src-tauri/src/app_state.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/shell.rs`, `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/integration.rs`
- Modify: `src/shared/types.ts`

- [ ] **Step 1: Failing test for the new settings fields** (in `settings.rs` tests)

```rust
    #[test]
    fn desk_position_defaults_to_unset_and_keeps_any_value() {
        let s = Settings::default();
        assert_eq!((s.triggers.desk_x, s.triggers.desk_y), (-1.0, -1.0));
        let l = from_value(json!({ "triggers": { "desk_x": -1200.0, "desk_y": 40.0 } }));
        assert_eq!((l.settings.triggers.desk_x, l.settings.triggers.desk_y), (-1200.0, 40.0));
        assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    }
```

Run `cargo test -j 2 -- settings::` and see it fail to compile.

- [ ] **Step 2: Add the fields**

In `Triggers`, add:

```rust
    /// Desk pad top-left, physical px; -1 = not placed yet (bottom-right of the work area).
    pub desk_x: f64,
    pub desk_y: f64,
```

with defaults `desk_x: -1.0, desk_y: -1.0` in `Default`. Write the literals as `-1.0` so they serialize as f64, which `overlay` needs to accept negative and fractional values. In `types.ts`, add `desk_x: number; desk_y: number` to `Settings["triggers"]`. Run `cargo test -j 2 -- settings::`: PASS.

- [ ] **Step 3: Move the `set_settings` logic into `AppState::update_settings`**

In `app_state.rs`:

```rust
impl AppState {
    /// Apply a JSON patch, save it, and return (before, after). The job queue is pumped
    /// so a higher concurrency starts queued jobs at once.
    pub fn update_settings(
        &self,
        patch: serde_json::Value,
    ) -> Result<(settings::Settings, settings::Settings), String> {
        let mut current = lock(&self.settings);
        let before = current.settings.clone();
        let next = settings::apply_patch(&current, patch);
        for w in &next.warnings {
            log::warn!("settings: {w}");
        }
        settings::save(&self.paths.settings(), &next).map_err(|e| e.to_string())?;
        *current = next;
        let after = current.settings.clone();
        drop(current); // pump reads settings through the same mutex
        self.jobs.pump();
        Ok((before, after))
    }

    pub fn settings_now(&self) -> settings::Settings {
        lock(&self.settings).settings.clone()
    }
}
```

Replace `ipc::set_settings` with:

```rust
#[tauri::command]
pub async fn set_settings(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    let handle = app.clone();
    let (before, after) =
        blocking(move || handle.state::<AppState>().update_settings(patch)).await?;
    crate::integration::settings_changed(&app, &before, &after);
    Ok(after)
}
```

Remove the now-unused imports (`settings` module import in `ipc.rs` if unused).

- [ ] **Step 4: Write `integration.rs`**

```rust
//! Applies settings to the world outside Recast's window: the Explorer verb and the
//! Run value (spec §6.10, §4.5), and (Task 7) the trigger windows and the shake hook.
use tauri::AppHandle;

use crate::platform;
use crate::settings::Settings;
use crate::shell;

/// Release builds always; a dev build only with RECAST_DEV_INTEGRATION=1, so `cargo run`
/// never points Explorer or the Run key at target\debug (deviation 6).
pub fn registry_allowed() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("RECAST_DEV_INTEGRATION").is_some()
}

/// Bring the Explorer verb and the Run value in line with `s`. Re-run at every start so
/// the paths follow the exe after an update or a move.
pub fn sync_registry(s: &Settings) {
    if !registry_allowed() {
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => {
            log::warn!("shell: exe path unknown: {e}");
            return;
        }
    };
    if s.shell.explorer_entry {
        for e in shell::verb_entries(&exe) {
            if let Err(err) = platform::hkcu_set_string(&e.key, e.name, &e.data) {
                log::warn!("shell: writing {}: {err}", e.key);
            }
        }
    } else if let Err(err) = platform::hkcu_delete_tree(shell::VERB_KEY) {
        log::warn!("shell: removing the Explorer entry: {err}");
    }
    let run = if s.app.start_with_windows {
        platform::hkcu_set_string(shell::RUN_KEY, Some(shell::RUN_VALUE), &shell::run_command(&exe))
    } else {
        platform::hkcu_delete_value(shell::RUN_KEY, shell::RUN_VALUE)
    };
    if let Err(err) = run {
        log::warn!("shell: start with Windows: {err}");
    }
}

/// Called after every settings change from the UI.
pub fn settings_changed(_app: &AppHandle, before: &Settings, after: &Settings) {
    if before.shell != after.shell || before.app.start_with_windows != after.app.start_with_windows {
        sync_registry(after);
    }
}
```

Add `pub mod integration;` to `lib.rs`.

- [ ] **Step 5: Forwarding at runtime** (append to `shell.rs`, above the tests)

```rust
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::lockutil::lock;

/// Paths forwarded by later processes, waiting for the debounce.
#[derive(Default)]
pub struct Forwarder {
    inner: Mutex<(Collector, bool)>, // (paths, a flush thread is waiting)
}

/// Queue `paths` for one click-mode pad, opened once the arrivals stop (§6.10).
pub fn forward(app: &AppHandle, paths: Vec<String>) {
    let Some(st) = app.try_state::<Forwarder>() else {
        crate::pad::open(app, paths, crate::pad::PadMode::Click);
        return;
    };
    let mut g = lock(&st.inner);
    g.0.push(paths, Instant::now());
    if g.1 {
        return; // the waiting thread will see the new deadline
    }
    g.1 = true;
    drop(g);
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let st = app.state::<Forwarder>();
            let mut g = lock(&st.inner);
            let now = Instant::now();
            match g.0.deadline() {
                None => {
                    g.1 = false;
                    return;
                }
                Some(d) if d > now => {
                    drop(g);
                    std::thread::sleep(d - now);
                }
                Some(_) => {
                    let paths = g.0.take_if_due(now).unwrap_or_default();
                    g.1 = false;
                    drop(g);
                    log::info!("shell: opening the pad with {} forwarded files", paths.len());
                    crate::pad::open(&app, paths, crate::pad::PadMode::Click);
                    return;
                }
            }
        }
    });
}
```

- [ ] **Step 6: The launch flow**
- In `tauri.conf.json`, set the companion window's `"visible": false`.
- In `lib.rs`, replace the single-instance plugin init with:

```rust
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let launch = shell::parse_args(&argv, std::path::Path::new(&cwd));
            if launch.pad.is_empty() {
                tray::show_companion(app)
            } else {
                shell::forward(app, launch.pad)
            }
        }))
```

In `setup`, after `tray::build(app)?;`:

```rust
            app.manage(shell::Forwarder::default());
            let state = app.state::<app_state::AppState>();
            integration::sync_registry(&state.settings_now());
            let argv: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            let launch = shell::parse_args(&argv, &cwd);
            if !launch.pad.is_empty() {
                shell::forward(app.handle(), launch.pad);
            } else if !launch.autostart {
                tray::show_companion(app.handle());
            }
```

Add `use tauri::Manager;` if it isn't already imported (it is: `use tauri::{Emitter, Manager};`).

- [ ] **Step 7: Verify**

Run `cd src-tauri && cargo test -j 2 && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`, then `npm run build`.
Expected: everything passes. Then check by hand. `npm run tauri dev` should open the companion as before, because a plain launch shows it.

With the app running, from Git Bash at the repo root:

```bash
"src-tauri/target/debug/recast.exe" --pad "$PWD/README.md"
```

The pad should open at the cursor in click mode for README.md, after about 300 ms. The second process forwards and exits. Stop only the dev process you started.

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri/src src/shared/types.ts src-tauri/tauri.conf.json
git commit -m "feat(shell): forward Explorer launches to one pad; Run and verb registry sync; companion hidden on autostart"
```

---

### Task 4: Pad controller: anchors, the shake probe, mode tracking, release grace

**Files:**
- Modify: `src-tauri/src/pad.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/pad.json`
- Modify: `src/shared/types.ts`, `src/shared/ipc.ts`, `src/pad/PadApp.tsx`

- [ ] **Step 1: Failing tests for the new `Inner` bookkeeping** (append to `pad.rs` tests; `req()` gains `probe: false`)

```rust
    fn probe() -> PadOpen {
        PadOpen { probe: true, mode: PadMode::Drag, ..req() }
    }

    #[test]
    fn mode_and_paths_follow_the_current_open() {
        let mut s = Inner { ready: true, ..Default::default() };
        assert!(!s.is_open_with(&[]));
        let g = s.begin(PadOpen { paths: vec!["a".into()], mode: PadMode::Drag, ..req() }).unwrap().generation;
        assert!(s.is_open_with(&["a".to_string()]));
        assert!(s.dragging(g));
        assert!(s.shown_focused(g), "a drop turns the session into click mode");
        assert!(!s.dragging(g));
        assert!(s.begin_close(Some(g)));
        assert!(!s.is_open_with(&["a".to_string()]));
        assert!(!s.shown_focused(g), "stale");
    }

    #[test]
    fn a_probe_needs_confirming_and_only_shake_opens_get_release_grace() {
        let mut s = Inner { ready: true, ..Default::default() };
        let g = s.begin(probe()).unwrap().generation;
        assert!(s.unconfirmed_probe(g));
        assert_eq!(s.shake_drag(), Some(g));
        assert!(s.confirm(g));
        assert!(!s.unconfirmed_probe(g));
        assert_eq!(s.shake_drag(), Some(g), "still a shake-opened drag after confirming");
        assert!(!s.confirm(g), "confirming twice is a no-op");
        let h = s.begin(PadOpen { mode: PadMode::Drag, ..req() }).unwrap().generation;
        assert!(!s.unconfirmed_probe(h));
        assert_eq!(s.shake_drag(), None, "a handle or desk open has no release grace");
        assert!(!s.confirm(g), "stale generation");
    }
```

Run `cargo test -j 2 -- pad::` and see the compile errors.

- [ ] **Step 2: Implement in `pad.rs`**
1. `PadOpen` gains `pub probe: bool` with the doc comment "A shake probe: drawn only once a file drag enters it (deviation 1)."
2. `Inner` gains:

```rust
    /// Mode of the open pad; None while closed.
    mode: Option<PadMode>,
    /// Paths of the current open.
    paths: Vec<String>,
    /// A shake probe that no file drag has entered yet.
    probe: bool,
    /// The current open came from a shake (gets the 1.5 s release grace).
    from_shake: bool,
```

3. Methods:

```rust
    fn begin(&mut self, mut req: PadOpen) -> Option<PadOpen> {
        self.generation += 1;
        req.generation = self.generation;
        self.mode = Some(req.mode);
        self.paths = req.paths.clone();
        self.probe = req.probe;
        self.from_shake = req.probe;
        if self.ready {
            self.pending = None;
            Some(req)
        } else {
            self.pending = Some(req);
            None
        }
    }

    fn end(&mut self) {
        self.generation += 1;
        self.pending = None;
        self.mode = None;
        self.paths.clear();
        self.probe = false;
        self.from_shake = false;
    }

    /// `pad_show(generation)`: allowed only for the current open; the pad is now a
    /// focused click-mode pad.
    fn shown_focused(&mut self, generation: u64) -> bool {
        if !self.is_current(generation) || self.mode.is_none() {
            return false;
        }
        self.mode = Some(PadMode::Click);
        self.probe = false;
        true
    }

    fn is_open_with(&self, paths: &[String]) -> bool {
        self.mode.is_some() && self.paths == paths
    }

    fn dragging(&self, generation: u64) -> bool {
        self.is_current(generation) && self.mode == Some(PadMode::Drag)
    }

    fn unconfirmed_probe(&self, generation: u64) -> bool {
        self.is_current(generation) && self.probe
    }

    fn confirm(&mut self, generation: u64) -> bool {
        if self.unconfirmed_probe(generation) {
            self.probe = false;
            true
        } else {
            false
        }
    }

    /// The generation of a shake-opened pad still in drag mode.
    fn shake_drag(&self) -> Option<u64> {
        (self.from_shake && self.mode == Some(PadMode::Drag)).then_some(self.generation)
    }
```

Delete `allows_show`. Update the existing test `show_after_close_is_a_no_op_and_an_unconditional_close_always_closes` to use `shown_focused` in place of `allows_show`.

4. Public API (replace `open` and `open_now`; `show` uses `shown_focused`):

```rust
pub const PROBE_MS: u64 = 300;
pub const RELEASE_GRACE_MS: u64 = 1500;

/// Open at the cursor.
pub fn open(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    open_at(app, paths, mode, None);
}

/// Open centred on `anchor` (physical px), or on the cursor when None.
pub fn open_at(app: &AppHandle, paths: Vec<String>, mode: PadMode, anchor: Option<(i32, i32)>) {
    log::info!("pad: open ({mode:?}, {} files)", paths.len());
    on_main(app, move |app| {
        open_now(app, paths, mode, anchor, false);
    });
}

/// A shake was detected: show an invisible drag-mode pad under the cursor. It is drawn
/// only if a file drag enters it (`confirm_drag`), and hidden after PROBE_MS otherwise.
pub fn open_probe(app: &AppHandle) {
    on_main(app, |app| {
        let Some(g) = open_now(app, vec![], PadMode::Drag, None, true) else {
            return;
        };
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(PROBE_MS));
            on_main(&app, move |app| {
                let unconfirmed = lock(&app.state::<PadState>().inner).unconfirmed_probe(g);
                if unconfirmed {
                    close_now(app, Some(g));
                }
            });
        });
    });
}

/// `pad_drag_entered(generation)`: a file drag reached the probe.
pub fn confirm_drag(app: &AppHandle, generation: u64) {
    on_main(app, move |app| {
        lock(&app.state::<PadState>().inner).confirm(generation);
    });
}

/// The left button went up after a shake: close a shake-opened pad that is still
/// waiting for a drop 1.5 s later (§4.1).
pub fn release_grace(app: &AppHandle) {
    on_main(app, |app| {
        let Some(g) = lock(&app.state::<PadState>().inner).shake_drag() else {
            return;
        };
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(RELEASE_GRACE_MS));
            on_main(&app, move |app| {
                let still = lock(&app.state::<PadState>().inner).dragging(g);
                if still {
                    close_now(app, Some(g));
                }
            });
        });
    });
}

/// The pad is open (any mode). Cheap; callable from any thread.
pub fn is_open(app: &AppHandle) -> bool {
    app.try_state::<PadState>()
        .is_some_and(|st| lock(&st.inner).mode.is_some())
}

/// The pad is open with exactly these files.
pub fn is_open_with(app: &AppHandle, paths: &[String]) -> bool {
    app.try_state::<PadState>()
        .is_some_and(|st| lock(&st.inner).is_open_with(paths))
}
```

`open_now(app, paths, mode, anchor, probe) -> Option<u64>` is the old body with these changes:
- Placement uses `let at = anchor.map(|(x, y)| PhysicalPosition::new(x as f64, y as f64)).unwrap_or(cursor);`, and `at` replaces `cursor` for `monitor_from_point` and the anchor.
- `PadOpen` gets `probe`.
- After `inner.begin(req)`, read `let g = inner.generation;`.
- It returns `Some(g)` at the end, or `None` when the window is missing.

`show(app, generation)` uses `if !inner.shown_focused(generation) { return; }` (the guard becomes `let mut inner`).

- [ ] **Step 3: The command**

In `ipc.rs`:

```rust
/// The pad's drag-enter for a shake probe: keep it (spec §4.1, deviation 1).
#[tauri::command]
pub async fn pad_drag_entered(app: AppHandle, generation: u64) -> Result<(), String> {
    crate::pad::confirm_drag(&app, generation);
    Ok(())
}
```

Register it in `lib.rs` `generate_handler!`, add `"pad_drag_entered"` to `build.rs` `COMMANDS`, and add `"allow-pad-drag-entered"` to `capabilities/pad.json`.

- [ ] **Step 4: Front end**
- `types.ts`: `PadOpen` gains `probe: boolean;`.
- `ipc.ts`: `padDragEntered: (generation: number) => invoke<void>("pad_drag_entered", { generation }),`.

In `PadApp.tsx`:
1. `Session` gains `probe: boolean;`.
2. In `begin`, compute `const probe = req.probe && req.paths.length === 0;`. Skip the capture when probing: change the capture branch's condition to `req.reduced_transparency || probe ? Promise.resolve(null) : …`, and add `probe` to the `next` session object.
3. Every `begin({...})` call site adds `probe: false`. There are two: the `enter` re-begin and `dropRef`'s reopen.
4. In the drag-drop `enter` branch, right after the `if (!s || s.generation !== openGen.current) return;` line, insert:

```ts
        if (s.probe) {
          // A text or link drag carries no paths: let the probe time out (Rust hides it).
          if (p.paths.length === 0) return;
          ipc.padDragEntered(s.generation).catch(() => {});
        }
```

   The existing code after it then re-begins with the dragged paths, and that session has `probe: false`.
5. Render: `if (!session || session.probe) return null;`.

- [ ] **Step 5: Verify**

Run `cd src-tauri && cargo test -j 2 -- pad:: && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`, then `npm test && npm run build`.
Expected: everything passes.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src src-tauri/build.rs src-tauri/capabilities/pad.json src/shared src/pad/PadApp.tsx
git commit -m "feat(pad): open_at anchors, invisible shake probe, mode tracking and release grace"
```

---

### Task 5: Shake detector and trigger geometry (pure)

**Files:**
- Create: `src-tauri/src/triggers/mod.rs` (for now only `pub mod geometry; pub mod shake;`), `src-tauri/src/triggers/shake.rs`, `src-tauri/src/triggers/geometry.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod triggers;`)

- [ ] **Step 1: Shake tests first** (`shake.rs`, tests module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A button-down at `x0`, then strokes to each x in `turns`, each taking `stroke_ms`,
    /// sampled every 8 ms (a 125 Hz mouse), then (optionally) the button-up.
    fn stream(x0: i32, turns: &[i32], stroke_ms: u64, up: bool) -> Vec<Sample> {
        let mut out = vec![Sample { t_ms: 1000, x: x0, y: 500, input: Input::Down }];
        let (mut t, mut x) = (1000u64, x0);
        for &to in turns {
            let steps = (stroke_ms / 8).max(1);
            for i in 1..=steps {
                let xi = x + (to - x) * i as i32 / steps as i32;
                out.push(Sample { t_ms: t + i * 8, x: xi, y: 500, input: Input::Move });
            }
            t += steps * 8;
            x = to;
        }
        if up {
            out.push(Sample { t_ms: t + 8, x, y: 500, input: Input::Up });
        }
        out
    }

    fn run(samples: &[Sample]) -> Vec<Shake> {
        let mut d = Detector::default();
        samples.iter().map(|s| d.feed(*s)).filter(|r| *r != Shake::None).collect()
    }

    #[test]
    fn a_quick_shake_arms_once_then_reports_the_release() {
        let s = stream(500, &[580, 500, 580, 500, 580, 500], 90, true);
        assert_eq!(run(&s), vec![Shake::Armed, Shake::Released]);
    }

    #[test]
    fn slow_waving_does_not_arm() {
        assert!(run(&stream(500, &[580, 500, 580, 500, 580], 320, true)).is_empty());
    }

    #[test]
    fn small_jitter_does_not_arm() {
        assert!(run(&stream(500, &[530, 500, 530, 500, 530, 500, 530], 60, true)).is_empty());
    }

    #[test]
    fn a_straight_drag_like_a_text_selection_does_not_arm() {
        assert!(run(&stream(100, &[900], 600, true)).is_empty());
    }

    #[test]
    fn vertical_shaking_does_not_arm() {
        let mut d = Detector::default();
        d.feed(Sample { t_ms: 0, x: 500, y: 500, input: Input::Down });
        let mut armed = false;
        for i in 0..60u64 {
            let y = if (i / 6) % 2 == 0 { 500 + (i % 6) as i32 * 20 } else { 600 - (i % 6) as i32 * 20 };
            armed |= d.feed(Sample { t_ms: i * 8, x: 500, y, input: Input::Move }) == Shake::Armed;
        }
        assert!(!armed);
    }

    #[test]
    fn moves_without_the_button_do_nothing_and_a_new_press_can_arm_again() {
        let mut d = Detector::default();
        for i in 0..40u64 {
            let x = if (i / 5) % 2 == 0 { 500 } else { 600 };
            assert_eq!(d.feed(Sample { t_ms: i * 8, x, y: 0, input: Input::Move }), Shake::None);
        }
        let s = stream(500, &[580, 500, 580, 500], 90, true);
        assert_eq!(s.iter().map(|x| d.feed(*x)).filter(|r| *r != Shake::None).count(), 2);
        let s = stream(500, &[580, 500, 580, 500], 90, false);
        assert_eq!(s.iter().map(|x| d.feed(*x)).filter(|r| *r == Shake::Armed).count(), 1);
    }

    #[test]
    fn a_click_without_a_shake_reports_nothing() {
        let mut d = Detector::default();
        assert_eq!(d.feed(Sample { t_ms: 0, x: 1, y: 1, input: Input::Down }), Shake::None);
        assert_eq!(d.feed(Sample { t_ms: 50, x: 1, y: 1, input: Input::Up }), Shake::None);
    }
}
```

Run `cargo test -j 2 -- triggers::shake` and see it fail to compile.

- [ ] **Step 2: Implement `shake.rs`**

```rust
//! Shake-while-dragging (spec §4.1): with the left button held, ≥3 horizontal direction
//! reversals within 500 ms, each stroke ≥40 px, arm the trigger. Pure; the mouse hook
//! feeds it (triggers/mod.rs).
use std::collections::VecDeque;

pub const MIN_STROKE_PX: i32 = 40;
pub const WINDOW_MS: u64 = 500;
pub const REVERSALS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down,
    Move,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub t_ms: u64,
    pub x: i32,
    pub y: i32,
    pub input: Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shake {
    None,
    /// A shake: open the probe pad. At most once per press.
    Armed,
    /// The button went up after this press armed.
    Released,
}

#[derive(Default, Debug)]
pub struct Detector {
    down: bool,
    fired: bool,
    /// -1 moving left, +1 right, 0 not yet moving far enough.
    dir: i32,
    /// x where the current stroke started.
    anchor: i32,
    /// Farthest x of the current stroke.
    extreme: i32,
    reversals: VecDeque<u64>,
}

impl Detector {
    pub fn feed(&mut self, s: Sample) -> Shake {
        match s.input {
            Input::Down => {
                *self = Detector { down: true, anchor: s.x, extreme: s.x, ..Default::default() };
                Shake::None
            }
            Input::Up => {
                let released = self.down && self.fired;
                *self = Detector::default();
                if released { Shake::Released } else { Shake::None }
            }
            Input::Move if self.down && !self.fired => self.step(s.t_ms, s.x),
            Input::Move => Shake::None,
        }
    }

    fn step(&mut self, t: u64, x: i32) -> Shake {
        if self.dir == 0 {
            let d = x - self.anchor;
            if d.abs() >= MIN_STROKE_PX {
                self.dir = d.signum();
                self.extreme = x;
            }
            return Shake::None;
        }
        if (x - self.extreme) * self.dir > 0 {
            self.extreme = x;
            return Shake::None;
        }
        if (self.extreme - x) * self.dir >= MIN_STROKE_PX {
            if (self.extreme - self.anchor) * self.dir >= MIN_STROKE_PX {
                self.reversals.push_back(t);
            }
            self.anchor = self.extreme;
            self.extreme = x;
            self.dir = -self.dir;
            while self.reversals.front().is_some_and(|&r| t.saturating_sub(r) > WINDOW_MS) {
                self.reversals.pop_front();
            }
            if self.reversals.len() >= REVERSALS {
                self.fired = true;
                return Shake::Armed;
            }
        }
        Shake::None
    }
}
```

Run `cargo test -j 2 -- triggers::shake`: PASS (7 tests). If `slow_waving` or `quick_shake` fails, fix the detector so the spec numbers hold. Don't change the test streams.

- [ ] **Step 3: Geometry tests** (`geometry.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn the_handle_sits_on_its_edge_at_its_position() {
        assert_eq!(handle_rect(Edge::Left, 0.5, WORK, 1.0), Rect { x: 0, y: 480, w: 6, h: 80 });
        assert_eq!(handle_rect(Edge::Right, 0.0, WORK, 1.0), Rect { x: 1914, y: 0, w: 6, h: 80 });
        assert_eq!(handle_rect(Edge::Top, 1.0, WORK, 1.0), Rect { x: 1840, y: 0, w: 80, h: 6 });
        assert_eq!(handle_rect(Edge::Bottom, 0.5, WORK, 1.5), Rect { x: 900, y: 1031, w: 120, h: 9 });
        let left_monitor = Rect { x: -2560, y: 0, w: 2560, h: 1400 };
        assert_eq!(handle_rect(Edge::Left, 0.5, left_monitor, 1.0).x, -2560);
    }

    #[test]
    fn the_pad_anchor_clears_the_handle() {
        let h = handle_rect(Edge::Left, 0.5, WORK, 1.0);
        assert_eq!(pad_anchor_beside(Edge::Left, h, 440, 1.0), (6 + 12 + 220, 520));
        let h = handle_rect(Edge::Right, 0.5, WORK, 2.0);
        assert_eq!(pad_anchor_beside(Edge::Right, h, 880, 2.0), (h.x - 24 - 440, h.y + h.h / 2));
        let h = handle_rect(Edge::Bottom, 0.5, WORK, 1.0);
        assert_eq!(pad_anchor_beside(Edge::Bottom, h, 440, 1.0), (960, h.y - 12 - 220));
    }

    #[test]
    fn desk_pad_defaults_bottom_right_and_saved_positions_stay_on_screen() {
        assert_eq!(desk_position(-1.0, -1.0, WORK, 1.0), (1920 - 96 - 24, 1040 - 96 - 24));
        assert_eq!(desk_position(100.0, 200.0, WORK, 1.0), (100, 200));
        assert_eq!(desk_position(5000.0, -300.0, WORK, 1.0), (1920 - 96, 0));
    }

    #[test]
    fn fullscreen_means_covering_the_monitor_and_never_the_shell() {
        let mon = Rect { x: 0, y: 0, w: 1920, h: 1080 };
        assert!(is_fullscreen(mon, mon, "Chrome_WidgetWin_1"));
        assert!(is_fullscreen(Rect { x: -8, y: -8, w: 1936, h: 1096 }, mon, "X"));
        assert!(!is_fullscreen(Rect { x: 0, y: 0, w: 1920, h: 1040 }, mon, "X"), "maximized, taskbar visible");
        assert!(!is_fullscreen(mon, mon, "Progman"));
        assert!(!is_fullscreen(mon, mon, "WorkerW"));
        assert!(!is_fullscreen(mon, mon, "Shell_TrayWnd"));
    }

    #[test]
    fn edges_parse_with_left_as_the_fallback() {
        assert_eq!(Edge::parse("right"), Edge::Right);
        assert_eq!(Edge::parse("diagonal"), Edge::Left);
    }
}
```

- [ ] **Step 4: Implement `geometry.rs`**

```rust
//! Where the edge handle, the desk pad and the pad go (spec §4.1), and what counts as
//! fullscreen (§6.7). Physical pixels throughout; `scale` is the monitor's DPI scale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn centre(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    /// From left, top, right, bottom.
    pub fn ltrb(l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect { x: l, y: t, w: r - l, h: b - t }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn parse(s: &str) -> Edge {
        match s {
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            _ => Edge::Left,
        }
    }
}

pub const HANDLE_LONG: f64 = 80.0;
pub const HANDLE_SHORT: f64 = 6.0;
pub const PAD_GAP: f64 = 12.0;
pub const DESK_SIZE: f64 = 96.0;
pub const DESK_MARGIN: f64 = 24.0;

fn px(v: f64, scale: f64) -> i32 {
    (v * scale).round().max(1.0) as i32
}

/// The handle on `edge` of `work`, `pos` (0..1) along it.
pub fn handle_rect(edge: Edge, pos: f64, work: Rect, scale: f64) -> Rect {
    let (long, short) = (px(HANDLE_LONG, scale), px(HANDLE_SHORT, scale));
    let along = |start: i32, len: i32| start + ((len - long).max(0) as f64 * pos.clamp(0.0, 1.0)).round() as i32;
    match edge {
        Edge::Left => Rect { x: work.x, y: along(work.y, work.h), w: short, h: long },
        Edge::Right => Rect { x: work.right() - short, y: along(work.y, work.h), w: short, h: long },
        Edge::Top => Rect { x: along(work.x, work.w), y: work.y, w: long, h: short },
        Edge::Bottom => Rect { x: along(work.x, work.w), y: work.bottom() - short, w: long, h: short },
    }
}

/// Centre for a `pad_px` pad beside the handle, so the handle stays free for a drop.
/// `pad::place` then clamps it into the work area.
pub fn pad_anchor_beside(edge: Edge, handle: Rect, pad_px: i32, scale: f64) -> (i32, i32) {
    let (gap, half) = (px(PAD_GAP, scale), pad_px / 2);
    let (cx, cy) = handle.centre();
    match edge {
        Edge::Left => (handle.right() + gap + half, cy),
        Edge::Right => (handle.x - gap - half, cy),
        Edge::Top => (cx, handle.bottom() + gap + half),
        Edge::Bottom => (cx, handle.y - gap - half),
    }
}

/// The desk pad's top-left: the saved spot kept inside `work`, or bottom-right if unset.
pub fn desk_position(saved_x: f64, saved_y: f64, work: Rect, scale: f64) -> (i32, i32) {
    let s = px(DESK_SIZE, scale);
    if saved_x < 0.0 && saved_y < 0.0 {
        let m = px(DESK_MARGIN, scale);
        return (work.right() - s - m, work.bottom() - s - m);
    }
    let clamp = |v: f64, lo: i32, len: i32| (v.round() as i32).clamp(lo, (lo + len - s).max(lo));
    (clamp(saved_x, work.x, work.w), clamp(saved_y, work.y, work.h))
}

const SHELL_CLASSES: [&str; 4] = ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

/// A window covering its whole monitor (not just the work area), and not the desktop or taskbar.
pub fn is_fullscreen(window: Rect, monitor: Rect, class: &str) -> bool {
    !SHELL_CLASSES.contains(&class)
        && window.x <= monitor.x
        && window.y <= monitor.y
        && window.right() >= monitor.right()
        && window.bottom() >= monitor.bottom()
}
```

Note: `desk_position` treats only `(-1, -1)`-style "both negative" as unset. A saved spot on a monitor to the left has a negative x but a non-negative y, and is clamped into `work`. `triggers/mod.rs` passes the work area of the monitor containing the saved point, or the primary's if there is none.

- [ ] **Step 5: Run and commit**

Run: `cd src-tauri && cargo test -j 2 -- triggers:: && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS (12 tests).

```bash
git add src-tauri/src/triggers src-tauri/src/lib.rs
git commit -m "feat(triggers): shake detector and handle/desk/fullscreen geometry (pure)"
```

---

### Task 6: Mouse hook, foreground watcher and foreground info in `platform`

**Files:**
- Modify: `src-tauri/Cargo.toml`: add the `windows` features `"Win32_UI_Accessibility"` and `"Win32_System_LibraryLoader"`.
- Modify: `src-tauri/src/platform/win.rs`

- [ ] **Step 1: Tests** (append to `win.rs` tests)

```rust
    #[test]
    fn the_mouse_hook_starts_and_stops_cleanly() {
        let hook = MouseHook::start(Box::new(|_e| {})).unwrap();
        drop(hook);
        // A second hook after the first is gone works too.
        drop(MouseHook::start(Box::new(|_e| {})).unwrap());
    }

    #[test]
    fn foreground_info_reads_without_panicking() {
        if let Some(f) = foreground_info() {
            assert!(f.monitor[2] > f.monitor[0] && f.monitor[3] > f.monitor[1]);
        }
    }
```

- [ ] **Step 2: Implement** (add to `win.rs`; extend the imports as needed)

```rust
use std::sync::{Mutex, OnceLock, PoisonError, mpsc};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EVENT_SYSTEM_FOREGROUND, GetClassNameW, GetDesktopWindow, GetForegroundWindow,
    GetMessageW, GetShellWindow, GetWindowRect, MONITORINFOF_PRIMARY, MSG, MSLLHOOKSTRUCT,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_QUIT, WM_TIMER,
};

/// The foreground window and its monitor, as left, top, right, bottom.
#[derive(Debug, Clone)]
pub struct ForegroundInfo {
    pub window: [i32; 4],
    pub monitor: [i32; 4],
    pub primary: bool,
    pub class: String,
}

pub fn foreground_info() -> Option<ForegroundInfo> {
    unsafe {
        let h = GetForegroundWindow();
        if h.is_invalid() || h == GetDesktopWindow() || h == GetShellWindow() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).ok()?;
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(m, &mut mi).as_bool() {
            return None;
        }
        let mut buf = [0u16; 128];
        let n = GetClassNameW(h, &mut buf).max(0) as usize;
        let mr = mi.rcMonitor;
        Some(ForegroundInfo {
            window: [r.left, r.top, r.right, r.bottom],
            monitor: [mr.left, mr.top, mr.right, mr.bottom],
            primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
            class: String::from_utf16_lossy(&buf[..n]),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseInput {
    LeftDown,
    LeftUp,
    Move,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// GetTickCount-style milliseconds.
    pub t_ms: u32,
    pub x: i32,
    pub y: i32,
    pub input: MouseInput,
}

type MouseSink = Box<dyn FnMut(MouseEvent) + Send>;
static MOUSE_SINK: Mutex<Option<MouseSink>> = Mutex::new(None);

unsafe extern "system" fn ll_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let input = match wparam.0 as u32 {
            WM_MOUSEMOVE => Some(MouseInput::Move),
            WM_LBUTTONDOWN => Some(MouseInput::LeftDown),
            WM_LBUTTONUP => Some(MouseInput::LeftUp),
            _ => None,
        };
        // Never block the system's mouse: skip the event if the sink is busy.
        if let Some(input) = input
            && let Ok(mut sink) = MOUSE_SINK.try_lock()
            && let Some(f) = sink.as_mut()
        {
            let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            f(MouseEvent { t_ms: info.time, x: info.pt.x, y: info.pt.y, input });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// A global low-level mouse hook on its own thread (spec §6.7). Dropping it unhooks.
/// One at a time: starting a second replaces the sink.
pub struct MouseHook {
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

impl MouseHook {
    pub fn start(sink: MouseSink) -> io::Result<MouseHook> {
        *MOUSE_SINK.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
        let (tx, rx) = mpsc::channel::<Result<u32, String>>();
        let join = std::thread::Builder::new()
            .name("recast-mouse-hook".into())
            .spawn(move || unsafe {
                let mut msg = MSG::default();
                // Create this thread's message queue before anyone posts WM_QUIT to it.
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse), module, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = tx.send(Ok(GetCurrentThreadId()));
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                let _ = UnhookWindowsHookEx(hook);
            })?;
        match rx.recv() {
            Ok(Ok(thread_id)) => Ok(MouseHook { thread_id, join: Some(join) }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(io::Error::other(e))
            }
            Err(_) => Err(io::Error::other("the mouse hook thread exited")),
        }
    }
}

impl Drop for MouseHook {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        *MOUSE_SINK.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

static FOREGROUND_SINK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(f) = FOREGROUND_SINK.get() {
        f();
    }
}

/// Call `f` whenever the foreground window changes and once a second (spec §6.7), on a
/// dedicated thread, for the life of the process. Only the first call starts anything.
pub fn watch_foreground(f: Box<dyn Fn() + Send + Sync>) -> io::Result<()> {
    if FOREGROUND_SINK.set(f).is_err() {
        return Err(io::Error::other("the foreground watcher is already running"));
    }
    std::thread::Builder::new()
        .name("recast-foreground".into())
        .spawn(|| unsafe {
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let _timer = SetTimer(None, 0, 1000, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_TIMER
                    && let Some(f) = FOREGROUND_SINK.get()
                {
                    f();
                }
            }
            let _ = UnhookWinEvent(hook);
        })?;
    Ok(())
}
```

Notes for the implementer:
- `SetTimer(None, …, None)` with no window posts `WM_TIMER` to the thread's queue, which `GetMessageW` returns. That's why it is matched there rather than dispatched.
- `WINEVENT_OUTOFCONTEXT` callbacks arrive on this thread during `GetMessageW`.
- `HINSTANCE(m.0)`: `GetModuleHandleW` returns an `HMODULE`. Convert it with whatever the 0.62 types offer (`m.into()` may work).
- In 0.62, `MSLLHOOKSTRUCT.time` is a `u32`.

- [ ] **Step 3: Run and commit**

Run: `cd src-tauri && cargo test -j 2 -- platform:: && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS.

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/platform/win.rs
git commit -m "feat(platform): low-level mouse hook thread, foreground watcher, foreground info"
```

---

### Task 7: The handle and desk-pad windows, trigger wiring, and removing the stand-ins

**Files:**
- Create: `widgets.html`, `src/widgets/main.tsx`, `src/widgets/Widget.tsx`, `src/widgets/widgets.css`, `src-tauri/capabilities/widgets.json`
- Modify: `vite.config.ts`, `src-tauri/tauri.conf.json`, `src-tauri/src/triggers/mod.rs`, `src-tauri/src/integration.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/ipc.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/default.json`
- Delete: `src-tauri/permissions/autogenerated/debug_open_pad.toml`

- [ ] **Step 1: The two windows** (append to `app.windows` in `tauri.conf.json`)

```json
      {
        "label": "handle",
        "title": "Recast handle",
        "url": "widgets.html",
        "width": 6,
        "height": 80,
        "resizable": false,
        "decorations": false,
        "transparent": true,
        "shadow": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": false,
        "focus": false,
        "focusable": false,
        "dragDropEnabled": true
      },
      {
        "label": "deskpad",
        "title": "Recast desk pad",
        "url": "widgets.html",
        "width": 96,
        "height": 96,
        "resizable": false,
        "maximizable": false,
        "decorations": false,
        "transparent": true,
        "shadow": false,
        "alwaysOnBottom": true,
        "skipTaskbar": true,
        "visible": false,
        "focus": false,
        "dragDropEnabled": true
      }
```

`capabilities/widgets.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "widgets",
  "description": "Edge handle and desk pad: events, and dragging the desk pad. No app commands.",
  "windows": ["handle", "deskpad"],
  "permissions": ["core:default", "core:window:allow-start-dragging"]
}
```

- [ ] **Step 2: The page**

`widgets.html` is a copy of `pad.html` with title "Recast widget", body class `widget-body`, and script `/src/widgets/main.tsx`. In `vite.config.ts`, add `widgets: "widgets.html"` to `rollupOptions.input`.

`src/widgets/main.tsx`:

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Widget } from "./Widget";
import "./widgets.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Widget />
  </StrictMode>,
);
```

`src/widgets/Widget.tsx`:

```tsx
/** The edge handle and the desk pad (spec §4.1). Rust handles their drag-drop events. */
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function Widget() {
  const [label] = useState(() => getCurrentWindow().label);
  const [hover, setHover] = useState(false);
  const [highlight, setHighlight] = useState(false);
  useEffect(() => {
    const offs = [
      listen<boolean>("widget://hover", (e) => setHover(e.payload)),
      listen<boolean>("widget://highlight", (e) => setHighlight(e.payload)),
    ];
    return () => offs.forEach((o) => o.then((f) => f()));
  }, []);
  const cls = (base: string) => [base, hover && "hover", highlight && "highlight"].filter(Boolean).join(" ");
  if (label === "deskpad") {
    return (
      <div className={cls("desk")} data-tauri-drag-region title="Drop files here to convert them">
        <img src="/icon.svg" alt="" draggable={false} />
      </div>
    );
  }
  return <div className={cls("handle")} title="Drag a file here" />;
}
```

`src/widgets/widgets.css`:

```css
html, body, #root { margin: 0; width: 100%; height: 100%; background: transparent; overflow: hidden; }
.handle {
  width: 100%; height: 100%; box-sizing: border-box; border-radius: 3px;
  background: rgba(255, 255, 255, 0.42);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.6), 0 0 0 1px rgba(0, 0, 0, 0.18);
  transition: background 0.15s;
}
.handle.hover { background: rgba(10, 132, 255, 0.85); }
.handle.highlight { animation: pulse 1.2s ease-in-out infinite; }
@keyframes pulse { 50% { background: rgba(10, 132, 255, 0.9); } }
.desk {
  width: 100%; height: 100%; box-sizing: border-box; border-radius: 50%;
  background: rgba(255, 255, 255, 0.18);
  box-shadow: inset 0 0 0 1.2px rgba(255, 255, 255, 0.7), inset 0 0 18px rgba(255, 255, 255, 0.25);
  display: grid; place-items: center; cursor: grab; transition: background 0.15s;
}
.desk img { width: 44px; height: 44px; pointer-events: none; }
.desk.hover { background: rgba(10, 132, 255, 0.45); }
@media (prefers-reduced-motion: reduce) {
  .handle.highlight { animation: none; background: rgba(10, 132, 255, 0.9); }
}
```

- [ ] **Step 3: `triggers/mod.rs`**

```rust
//! The ways into the pad (spec §4.1, §6.7): edge handle, desk pad and shake.
pub mod geometry;
pub mod shake;

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, DragDropEvent, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::app_state::AppState;
use crate::lockutil::lock;
use crate::pad::{self, PadMode};
use crate::platform;
use crate::settings::Settings;
use geometry::{Edge, Rect};

pub const HANDLE: &str = "handle";
pub const DESK: &str = "deskpad";

#[derive(Default)]
pub struct TriggerState {
    inner: Mutex<Inner>,
    desk_save: AtomicU64,
}

#[derive(Default)]
struct Inner {
    /// What the handle window was last set to (None: never).
    handle_shown: Option<bool>,
    /// The primary monitor's foreground window is fullscreen.
    fullscreen: bool,
    hook: Option<platform::MouseHook>,
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings_now()
}

/// Startup: create the trigger state, apply the settings, start the foreground watcher.
pub fn init(app: &AppHandle) {
    app.manage(TriggerState::default());
    sync(app, &settings(app));
    let h = app.clone();
    if let Err(e) = platform::watch_foreground(Box::new(move || tick(&h))) {
        log::warn!("triggers: foreground watcher: {e}");
    }
}

/// Apply trigger settings: place and show or hide the widgets; start or stop the hook.
pub fn sync(app: &AppHandle, s: &Settings) {
    let s2 = s.clone();
    let _ = app.run_on_main_thread({
        let app = app.clone();
        move || {
            place_handle(&app, &s2);
            refresh_handle(&app, &s2, true);
            place_desk(&app, &s2);
            set_visible(&app, DESK, s2.triggers.desk_pad);
        }
    });
    set_shake(app, s.triggers.shake);
}

fn primary_work(app: &AppHandle) -> Option<(Rect, f64)> {
    let m = app.primary_monitor().ok()??;
    let wa = m.work_area();
    Some((
        Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 },
        m.scale_factor(),
    ))
}

fn handle_geometry(app: &AppHandle, s: &Settings) -> Option<(Edge, Rect, f64)> {
    let (work, scale) = primary_work(app)?;
    let edge = Edge::parse(&s.triggers.edge);
    Some((edge, geometry::handle_rect(edge, s.triggers.edge_pos, work, scale), scale))
}

fn place_handle(app: &AppHandle, s: &Settings) {
    let (Some(w), Some((_, r, _))) = (app.get_webview_window(HANDLE), handle_geometry(app, s)) else {
        return;
    };
    let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = w.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
}

fn place_desk(app: &AppHandle, s: &Settings) {
    let Some(w) = app.get_webview_window(DESK) else { return };
    let (sx, sy) = (s.triggers.desk_x, s.triggers.desk_y);
    // The monitor holding the saved spot, else the primary.
    let target = app
        .monitor_from_point(sx.max(0.0), sy.max(0.0))
        .ok()
        .flatten()
        .filter(|_| sx >= 0.0 || sy >= 0.0)
        .map(|m| {
            let wa = m.work_area();
            (Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 }, m.scale_factor())
        })
        .or_else(|| primary_work(app));
    let Some((work, scale)) = target else { return };
    let (x, y) = geometry::desk_position(sx, sy, work, scale);
    let size = (geometry::DESK_SIZE * scale).round() as u32;
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(size, size));
}

/// Show without activating, or hide, a widget window. Like the pad (§6.7), shown behind
/// tao's back, so it is hidden through the OS as well.
fn set_visible(app: &AppHandle, label: &str, on: bool) {
    let Some(w) = app.get_webview_window(label) else { return };
    let Ok(h) = w.hwnd() else { return };
    let h = h.0 as isize;
    if on {
        platform::show_no_activate(h);
    } else {
        let _ = w.hide();
        platform::hide_window(h);
    }
}

/// Handle visible = setting on and no fullscreen app in front. `force` re-applies.
fn refresh_handle(app: &AppHandle, s: &Settings, force: bool) {
    let st = app.state::<TriggerState>();
    let mut i = lock(&st.inner);
    let want = s.triggers.edge_handle && !i.fullscreen;
    if force || i.handle_shown != Some(want) {
        i.handle_shown = Some(want);
        drop(i);
        set_visible(app, HANDLE, want);
    }
}

fn fullscreen_on(primary_only: bool) -> bool {
    platform::foreground_info().is_some_and(|f| {
        (!primary_only || f.primary)
            && geometry::is_fullscreen(
                Rect::ltrb(f.window[0], f.window[1], f.window[2], f.window[3]),
                Rect::ltrb(f.monitor[0], f.monitor[1], f.monitor[2], f.monitor[3]),
                &f.class,
            )
    })
}

/// Foreground change or the 1 s timer (watcher thread): auto-hide the handle over
/// fullscreen apps.
fn tick(app: &AppHandle) {
    let full = fullscreen_on(true);
    let st = app.state::<TriggerState>();
    let changed = {
        let mut i = lock(&st.inner);
        let c = i.fullscreen != full;
        i.fullscreen = full;
        c
    };
    if changed {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || refresh_handle(&app2, &settings(&app2), false));
    }
}

fn set_shake(app: &AppHandle, on: bool) {
    let st = app.state::<TriggerState>();
    let mut i = lock(&st.inner);
    if on == i.hook.is_some() {
        return;
    }
    if !on {
        i.hook = None; // Drop unhooks and joins the hook thread
        return;
    }
    let app = app.clone();
    let mut det = shake::Detector::default();
    let hook = platform::MouseHook::start(Box::new(move |e| {
        let input = match e.input {
            platform::MouseInput::LeftDown => shake::Input::Down,
            platform::MouseInput::LeftUp => shake::Input::Up,
            platform::MouseInput::Move => shake::Input::Move,
        };
        match det.feed(shake::Sample { t_ms: e.t_ms as u64, x: e.x, y: e.y, input }) {
            shake::Shake::Armed => on_shake(&app),
            shake::Shake::Released => pad::release_grace(&app),
            shake::Shake::None => {}
        }
    }));
    match hook {
        Ok(h) => i.hook = Some(h),
        Err(e) => log::warn!("shake: mouse hook failed: {e}"),
    }
}

/// Runs on the hook thread: keep it quick.
fn on_shake(app: &AppHandle) {
    if pad::is_open(app) || fullscreen_on(false) {
        return;
    }
    log::info!("shake: armed");
    pad::open_probe(app);
}

/// Where the pad goes for a drag over a widget: beside the handle, over the desk pad.
fn anchor_for(w: &tauri::Window) -> Option<(i32, i32)> {
    let (pos, size, scale) = (w.outer_position().ok()?, w.outer_size().ok()?, w.scale_factor().ok()?);
    let r = Rect { x: pos.x, y: pos.y, w: size.width as i32, h: size.height as i32 };
    if w.label() == HANDLE {
        let edge = Edge::parse(&settings(w.app_handle()).triggers.edge);
        let pad_px = (pad::SIZE * scale).round() as i32;
        Some(geometry::pad_anchor_beside(edge, r, pad_px, scale))
    } else {
        Some(r.centre())
    }
}

fn strings(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// Drag-drop on the handle or desk pad (spec §4.1): entering opens a drag-mode pad for
/// these files; dropping on the widget itself opens a click-mode pad.
pub fn on_drag(w: &tauri::Window, ev: &DragDropEvent) {
    let app = w.app_handle();
    let label = w.label().to_string();
    match ev {
        DragDropEvent::Enter { paths, .. } => {
            let paths = strings(paths);
            if paths.is_empty() {
                return; // text, links, virtual items (§4.1)
            }
            let _ = app.emit_to(&label, "widget://hover", true);
            if !pad::is_open_with(app, &paths) {
                pad::open_at(app, paths, PadMode::Drag, anchor_for(w));
            }
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit_to(&label, "widget://hover", false);
            let paths = strings(paths);
            if !paths.is_empty() {
                pad::open_at(app, paths, PadMode::Click, anchor_for(w));
            }
        }
        DragDropEvent::Leave => {
            let _ = app.emit_to(&label, "widget://hover", false);
        }
        _ => {}
    }
}

/// The desk pad was dragged: remember where, 500 ms after it stops.
pub fn desk_moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    let st = app.state::<TriggerState>();
    let mine = st.desk_save.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        if app.state::<TriggerState>().desk_save.load(Ordering::SeqCst) != mine {
            return;
        }
        let s = settings(&app);
        if (s.triggers.desk_x - pos.x as f64).abs() < 1.0 && (s.triggers.desk_y - pos.y as f64).abs() < 1.0 {
            return;
        }
        let patch = serde_json::json!({ "triggers": { "desk_x": pos.x as f64, "desk_y": pos.y as f64 } });
        if let Err(e) = app.state::<AppState>().update_settings(patch) {
            log::warn!("desk pad: saving its position: {e}");
        }
    });
}

/// First run (§4.6): make the handle pulse while the welcome points at it.
pub fn highlight(app: &AppHandle, on: bool) {
    let _ = app.emit_to(HANDLE, "widget://highlight", on);
}
```

- [ ] **Step 4: Wiring**

In `integration::settings_changed`, rename `_app` to `app` and append:

```rust
    if before.triggers != after.triggers {
        crate::triggers::sync(app, after);
    }
```

`Triggers` already derives `PartialEq`. A change to `desk_x`/`desk_y` therefore re-syncs. That's harmless, because `desk_moved` writes through `update_settings` directly, which doesn't call `settings_changed`.

In `lib.rs`:
- In `setup`, add `triggers::init(app.handle());` after `pad::init`.
- Replace the `on_window_event` closure:

```rust
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(ev)
                if matches!(window.label(), triggers::HANDLE | triggers::DESK) =>
            {
                triggers::on_drag(window, ev)
            }
            tauri::WindowEvent::Moved(pos) if window.label() == triggers::DESK => {
                triggers::desk_moved(window.app_handle(), *pos)
            }
            tauri::WindowEvent::CloseRequested { api, .. } => match window.label() {
                "companion" => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                pad::LABEL => {
                    api.prevent_close();
                    let _ = window.app_handle().emit_to(pad::LABEL, "pad://dismiss", ());
                    pad::close(window.app_handle());
                }
                // The widgets are reused for the whole run, like the pad.
                triggers::HANDLE | triggers::DESK => api.prevent_close(),
                _ => {}
            },
            _ => {}
        })
```

Add a `highlight_handle` command to `ipc.rs` (companion only):

```rust
#[tauri::command]
pub async fn highlight_handle(app: AppHandle, on: bool) -> Result<(), String> {
    crate::triggers::highlight(&app, on);
    Ok(())
}
```

Register it in `generate_handler!` and add it to `build.rs` `COMMANDS`. Add `"allow-highlight-handle"` to `capabilities/default.json`, and in `ipc.ts` add `highlightHandle: (on: boolean) => invoke<void>("highlight_handle", { on }),`.

- [ ] **Step 5: Remove the Plan 2 stand-ins**
- `tray.rs`: delete the `droppad` item, its menu entry and its match arm. Update the module doc comment to "Open, Convert files… and Quit".
- `ipc.rs`: delete `debug_open_pad`.
- `lib.rs` and `build.rs`: delete `debug_open_pad`.
- `capabilities/default.json`: delete `"allow-debug-open-pad"`.
- Delete `src-tauri/permissions/autogenerated/debug_open_pad.toml`.
- Search for leftovers: `grep -rn "debug_open_pad\|droppad\|Drop pad" src src-tauri/src src-tauri/capabilities` should print nothing.

- [ ] **Step 6: Verify, in the running app**

Run `cd src-tauri && cargo test -j 2 && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`, then `npm test && npm run build`.

Then `npm run tauri dev`. The companion, Activity and Library should work as before. Then check:
1. A thin handle is visible at the middle of the primary monitor's left edge.
   - Check its size: `tasklist` doesn't help, so either take a screenshot, or log `outer_size()` once in `place_handle` at debug level and read `%LOCALAPPDATA%\Recast\logs\recast.log`.
   - If the OS forced a larger minimum width, keep the window at whatever width it accepts, and make `.handle` draw a 6 px tab flush with the edge inside a transparent window. Record that in the report.
2. Maximize a browser and press F11 to go fullscreen: the handle disappears within a second. Leave fullscreen and it comes back.
3. Enable the desk pad by editing `%APPDATA%\Recast\settings.json` (`"desk_pad": true`) and restarting. The round widget appears at the bottom right. Drag it somewhere, restart, and it is still there.

A real Explorer drag onto the handle needs a human. List it for Task 12's QA checklist; don't try to fake it.

- [ ] **Step 7: Commit**

```bash
git add -A widgets.html vite.config.ts src/widgets src/shared/ipc.ts src-tauri
git commit -m "feat(triggers): edge handle and desk pad windows, shake hook, fullscreen auto-hide; remove the Drop pad stand-ins"
```

**→ Opus checkpoint review 1 (Tasks 1–7).**

---

### Task 8: Notification buttons

**Files:**
- Create: `src-tauri/src/toast.rs`
- Modify: `src-tauri/src/jobs/notify.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/app_state.rs`, `src-tauri/src/lib.rs`
- Modify: `src/companion/App.tsx`, `src/companion/Activity.tsx`, `src/shared/ipc.ts`

- [ ] **Step 1: Targets on the notifier** (tests first, in `notify.rs`)

Add to `notify.rs`:

```rust
/// What a notification is about, so its buttons and a click can act on it (spec §4.3).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A finished job: "Show in folder".
    Done(String),
    /// A failed job: "Details".
    Failed(String),
    /// A job that couldn't write its output: "Save to fallback folder" and "Details".
    DestFailed(String),
    /// A batch or a skip: a click opens Activity.
    Activity,
    /// An engine download that failed: "Retry".
    Engine(String),
}
```

`Notification` gains `pub target: Option<Target>`. Extend the tests:

```rust
    #[test]
    fn notifications_carry_what_their_buttons_act_on() {
        let mut n = Notifier::default();
        let mut done = ev("b", 1, "done");
        done.job_id = "j1".into();
        assert_eq!(n.on_event(&done).unwrap().target, Some(Target::Done("j1".into())));
        assert_eq!(n.on_event(&ev("c", 1, "failed")).unwrap().target, Some(Target::Failed("j".into())));
        let mut dest = ev("d", 1, "failed");
        dest.error = Some(JobError { code: ErrorCode::DestUnwritable, message: "x".into() });
        assert_eq!(n.on_event(&dest).unwrap().target, Some(Target::DestFailed("j".into())));
        let mut locked = ev("e", 1, "failed");
        locked.error = Some(JobError { code: ErrorCode::DestLocked, message: "x".into() });
        assert_eq!(n.on_event(&locked).unwrap().target, Some(Target::DestFailed("j".into())));
        for s in ["done", "done"] {
            let _ = n.on_event(&ev("f", 3, s));
        }
        assert_eq!(n.on_event(&ev("f", 3, "done")).unwrap().target, Some(Target::Activity));
    }
```

Set the targets in `on_event`:
- batch summaries (both returns): `Some(Target::Activity)`
- `"done"`: `Some(Target::Done(ev.job_id.clone()))`
- `"skipped"`: `Some(Target::Activity)`
- `"failed"`: `DestFailed` when `ev.error` has code `DestUnwritable` or `DestLocked`, otherwise `Failed`

Fix the existing test constructions that build a `Notification` or compare whole values. They compare fields, so this may only need `..` edits. Run `cargo test -j 2 -- jobs::notify`: PASS.

- [ ] **Step 2: `toast.rs` pure part, with tests**

```rust
//! Notifications with buttons (spec §4.3, §5.7, §7). The pure part maps a target and an
//! action to what Recast does; `init` and `show` wire it to tauri-plugin-notification.
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::{Action, ActionType, NotificationExt};
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
pub use crate::jobs::notify::Target;
use crate::lockutil::lock;

pub const DONE: &str = "recast-done";
pub const FAILED: &str = "recast-failed";
pub const DEST: &str = "recast-dest";
pub const ENGINE: &str = "recast-engine";

pub fn action_type(t: &Target) -> Option<&'static str> {
    match t {
        Target::Done(_) => Some(DONE),
        Target::Failed(_) => Some(FAILED),
        Target::DestFailed(_) => Some(DEST),
        Target::Engine(_) => Some(ENGINE),
        Target::Activity => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Open the companion at a section, optionally expanding one job.
    Navigate { section: &'static str, job_id: Option<String> },
    Reveal(String),
    Fallback(String),
    InstallEngine(String),
}

/// `action` is the button id, or `tap` for a click on the notification itself.
pub fn resolve(action: &str, t: &Target) -> Option<Act> {
    let nav = |section, job: Option<&String>| Act::Navigate { section, job_id: job.cloned() };
    match (action, t) {
        ("show", Target::Done(id)) => Some(Act::Reveal(id.clone())),
        ("fallback", Target::DestFailed(id)) => Some(Act::Fallback(id.clone())),
        ("retry", Target::Engine(id)) => Some(Act::InstallEngine(id.clone())),
        ("details" | "tap", Target::Failed(id) | Target::DestFailed(id)) => Some(nav("activity", Some(id))),
        ("tap", Target::Engine(_)) => Some(nav("library", None)),
        ("tap", Target::Done(id)) => Some(nav("activity", Some(id))),
        ("tap", Target::Activity) => Some(nav("activity", None)),
        _ => None,
    }
}

const KEEP: usize = 200;

/// Notification id → target, for the last 200 notifications.
pub struct Targets {
    next: i32,
    order: VecDeque<i32>,
    map: HashMap<i32, Target>,
}

impl Default for Targets {
    fn default() -> Self {
        Self { next: 1, order: VecDeque::new(), map: HashMap::new() }
    }
}

impl Targets {
    pub fn add(&mut self, t: Target) -> i32 {
        let id = self.next;
        self.next = self.next.checked_add(1).unwrap_or(1);
        self.map.insert(id, t);
        self.order.push_back(id);
        while self.order.len() > KEEP {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        id
    }

    pub fn get(&self, id: i32) -> Option<&Target> {
        self.map.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_to_actions() {
        let j = || "j1".to_string();
        assert_eq!(resolve("show", &Target::Done(j())), Some(Act::Reveal(j())));
        assert_eq!(resolve("fallback", &Target::DestFailed(j())), Some(Act::Fallback(j())));
        assert_eq!(resolve("retry", &Target::Engine("ffmpeg".into())), Some(Act::InstallEngine("ffmpeg".into())));
        assert_eq!(
            resolve("details", &Target::Failed(j())),
            Some(Act::Navigate { section: "activity", job_id: Some(j()) })
        );
        assert_eq!(
            resolve("tap", &Target::Engine("x".into())),
            Some(Act::Navigate { section: "library", job_id: None })
        );
        assert_eq!(resolve("show", &Target::Failed(j())), None, "no such button");
        assert_eq!(action_type(&Target::Activity), None);
        assert_eq!(action_type(&Target::DestFailed(j())), Some(DEST));
    }

    #[test]
    fn targets_keep_the_last_200() {
        let mut t = Targets::default();
        let first = t.add(Target::Activity);
        for _ in 0..200 {
            t.add(Target::Activity);
        }
        assert!(t.get(first).is_none());
        assert!(t.get(first + 200).is_some());
    }
}
```

Run `cargo test -j 2 -- toast::`: PASS.

- [ ] **Step 3: `toast.rs` Tauri part** (append above the tests)

```rust
pub struct Toasts(Mutex<Targets>);

/// Register the button sets and the action handler. Call in setup, before any
/// notification: the plugin shows buttons only once a handler exists.
pub fn init(app: &AppHandle) {
    app.manage(Toasts(Mutex::new(Targets::default())));
    let a = |id: &str, title: &str| Action::builder(id, title).build();
    let t = |id: &str, actions: Vec<Action>| ActionType::builder(id).actions(actions).build();
    let n = app.notification();
    if let Err(e) = n.register_action_types(vec![
        t(DONE, vec![a("show", "Show in folder")]),
        t(FAILED, vec![a("details", "Details")]),
        t(DEST, vec![a("fallback", "Save to fallback folder"), a("details", "Details")]),
        t(ENGINE, vec![a("retry", "Retry")]),
    ]) {
        log::warn!("notifications: registering buttons: {e}");
    }
    let h = app.clone();
    let _ = n.on_action(move |p| {
        let Some(id) = p.notification().map(|n| n.id()) else { return };
        let target = lock(&h.state::<Toasts>().0).get(id).cloned();
        if let Some(act) = target.and_then(|t| resolve(p.action_id(), &t)) {
            perform(&h, act);
        }
    });
}

/// Show a notification; `target` decides its buttons and what a click does.
pub fn show(app: &AppHandle, title: &str, body: &str, target: Option<Target>) {
    let mut b = app.notification().builder().title(title).body(body);
    if let Some(t) = target {
        if let Some(at) = action_type(&t) {
            b = b.action_type_id(at);
        }
        if let Some(st) = app.try_state::<Toasts>() {
            b = b.id(lock(&st.0).add(t));
        }
    }
    if let Err(e) = b.show() {
        log::warn!("notification: {e}");
    }
}

#[derive(serde::Serialize, Clone)]
struct Navigate {
    section: &'static str,
    job_id: Option<String>,
}

fn perform(app: &AppHandle, act: Act) {
    match act {
        Act::Navigate { section, job_id } => {
            crate::tray::show_companion(app);
            let _ = app.emit_to("companion", "companion://navigate", Navigate { section, job_id });
        }
        Act::Reveal(id) => {
            let st = app.state::<AppState>();
            match crate::ipc::job_output(&lock(&st.history), &id) {
                Ok(out) => {
                    if let Err(e) = app.opener().reveal_item_in_dir(out) {
                        log::warn!("show in folder: {e}");
                    }
                }
                Err(e) => log::warn!("show in folder: {e}"),
            }
        }
        Act::Fallback(id) => {
            let jobs = app.state::<AppState>().jobs.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs.retry(&id, true) {
                    log::warn!("save to fallback folder: {e}");
                }
            });
        }
        Act::InstallEngine(id) => {
            let engines = app.state::<AppState>().engines.clone();
            std::thread::spawn(move || {
                let _ = engines.install(&id, &mut |_| {});
            });
        }
    }
}
```

Make `ipc::job_output` `pub(crate)`. If `jobs.retry` returns a different `Result` type than `Result<_, String>`, adapt the `log::warn!`.

- [ ] **Step 4: Route every notification through `toast::show`**
- `ipc.rs` `TauriEvents::emit`: replace the builder chain with `crate::toast::show(&self.app, &n.title, &n.body, n.target);`. Remove the now-unused `NotificationExt` import.
- `app_state.rs` engine listener: replace the builder chain with `crate::toast::show(&handle, &format!("Couldn't download {name}"), error, Some(crate::jobs::notify::Target::Engine(id.clone())));`.
- `lib.rs`: add `pub mod toast;` and call `toast::init(app.handle());` in `setup` right after `app_state::setup(app)?;`.

- [ ] **Step 5: The companion follows `companion://navigate`**
- `ipc.ts`: `export const onNavigate = (cb: (p: { section: "activity" | "library"; job_id: string | null }) => void) => listen<{ section: "activity" | "library"; job_id: string | null }>("companion://navigate", (e) => cb(e.payload));`.
- `App.tsx`: hold `const [focusJob, setFocusJob] = useState<{ id: string; n: number } | null>(null);`. Add an effect that subscribes to `onNavigate` (unlisten in cleanup), calls `setSection(p.section)`, and when `p.job_id` is set calls `setFocusJob((f) => ({ id: p.job_id!, n: (f?.n ?? 0) + 1 }))`. Pass `focusJob` to `<Activity focusJob={focusJob} />`.
- `Activity.tsx`: `export function Activity({ focusJob }: { focusJob?: { id: string; n: number } | null })`. Pass `focused={focusJob?.id === j.id ? focusJob.n : 0}` to each `JobRow`. In `JobRow`, add `focused: number` to the props, plus:

```tsx
  const rowRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    if (!focused) return;
    setOpen(true);
    rowRef.current?.scrollIntoView({ block: "center" });
  }, [focused]);
```

and `ref={rowRef}` on the `<li>`. Import `useRef`.

- [ ] **Step 6: Verify and commit**

Run `cd src-tauri && cargo test -j 2 && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`, then `npm test && npm run build`.

In the running app, with the companion hidden or unfocused, convert a PNG to WebP through `recast.exe --pad <png>`. The toast shows **Show in folder**, and clicking it opens Explorer with the file selected. Failure buttons need a human; list them in the QA checklist.

```bash
git add -A src-tauri/src src/companion src/shared
git commit -m "feat(notify): Show in folder, Details, Save to fallback folder and engine Retry buttons"
```

---

### Task 9: Tray glyphs that follow the taskbar theme

**Files:**
- Create: `src-tauri/icons/tray-dark-taskbar.png`, `src-tauri/icons/tray-light-taskbar.png`
- Modify: `src-tauri/Cargo.toml` (tauri feature `image-png`), `src-tauri/src/tray.rs`, `src-tauri/src/triggers/mod.rs`

- [ ] **Step 1: Render the PNGs** (from the repo root)

```bash
T="$LOCALAPPDATA/Temp/recast-tray"; rm -rf "$T"
npx tauri icon assets/icon/tray-dark-taskbar.svg -o "$T/dark"
npx tauri icon assets/icon/tray-light-taskbar.svg -o "$T/light"
cp "$T/dark/32x32.png" src-tauri/icons/tray-dark-taskbar.png
cp "$T/light/32x32.png" src-tauri/icons/tray-light-taskbar.png
```

Open both PNGs with the Read tool and check that they show the glyph: white on transparent for the dark taskbar, dark for the light one. If `tauri icon` refuses the SVG, render 32 × 32 PNGs another way that needs no new project dependency, and say how in the report.

- [ ] **Step 2: Test, then implement** (`tray.rs`)

Add `"image-png"` to the `tauri` features in `Cargo.toml`.

```rust
const TRAY_DARK: &[u8] = include_bytes!("../icons/tray-dark-taskbar.png");
const TRAY_LIGHT: &[u8] = include_bytes!("../icons/tray-light-taskbar.png");

/// The glyph for the taskbar's theme (§12): white on a dark taskbar, dark on a light one.
pub fn glyph(light_taskbar: bool) -> tauri::Result<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(if light_taskbar { TRAY_LIGHT } else { TRAY_DARK })
}

pub fn set_theme(app: &AppHandle, light_taskbar: bool) {
    if let (Some(tray), Ok(icon)) = (app.tray_by_id("main"), glyph(light_taskbar)) {
        let _ = tray.set_icon(Some(icon));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_tray_glyphs_decode() {
        assert!(super::glyph(true).is_ok());
        assert!(super::glyph(false).is_ok());
    }
}
```

In `build`, replace `.icon(app.default_window_icon().cloned().expect("bundle icon"))` with `.icon(glyph(crate::platform::taskbar_is_light())?)`.

In `triggers/mod.rs`, `Inner` gains `light_taskbar: Option<bool>`, and `tick` gains:

```rust
    let light = platform::taskbar_is_light();
    let theme_changed = {
        let mut i = lock(&st.inner);
        let c = i.light_taskbar.is_some_and(|l| l != light);
        i.light_taskbar = Some(light);
        c
    };
    if theme_changed {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || crate::tray::set_theme(&app2, light));
    }
```

- [ ] **Step 3: Verify and commit**

Run `cd src-tauri && cargo test -j 2 -- tray:: && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`.

In the running app the tray shows the glyph. Switch Windows' "Choose your default Windows mode" between Dark and Light, and the glyph follows within a second.

```bash
git add src-tauri/icons/tray-*.png src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/tray.rs src-tauri/src/triggers/mod.rs
git commit -m "feat(tray): monochrome glyphs that follow the taskbar theme"
```

---

### Task 10: Open in Recast for mixed kinds

**Files:**
- Create: `src/pad/openInRecast.ts`, `src/pad/openInRecast.test.ts`, `src/companion/groups.ts`, `src/companion/groups.test.ts`
- Modify: `src/pad/PadApp.tsx`, `src/pad/usePad.ts`, `src/companion/Activity.tsx`, `src/companion/InlineRing.tsx`, `src/companion/App.tsx`, `src/companion/companion.css`, `src/shared/ipc.ts`
- Modify: `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/pad.json`

- [ ] **Step 1: Pure helpers, tests first**

`src/pad/openInRecast.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { OPEN_IN_RECAST_ID, withOpenInRecast } from "./openInRecast";
import type { PadModel } from "../shared/types";

const m = (over: Partial<PadModel>): PadModel => ({ files: [], presets: [], open_in_recast: false, ...over });

describe("withOpenInRecast", () => {
  it("adds the one segment only when the files are mixed and nothing covers them all", () => {
    expect(withOpenInRecast(m({ open_in_recast: true })).presets.map((p) => p.id)).toEqual([OPEN_IN_RECAST_ID]);
    expect(withOpenInRecast(m({ open_in_recast: false })).presets).toEqual([]);
  });
});
```

`src/pad/openInRecast.ts`:

```ts
/** Mixed kinds with no common preset (spec §5.6): one segment that opens Activity with the files grouped. */
import type { PadModel, PresetView } from "../shared/types";

export const OPEN_IN_RECAST_ID = "open-in-recast";

export const OPEN_IN_RECAST: PresetView = {
  id: OPEN_IN_RECAST_ID,
  label: "Open",
  description: "Open in Recast · convert each kind on its own",
  icon: "stack",
  engine: "",
  engine_name: "",
  engine_status: { state: "installed", version: "", update_available: false },
  min_files: 1,
  engine_size: 0,
  warning: null,
};

export function withOpenInRecast(model: PadModel): PadModel {
  return model.open_in_recast && model.presets.length === 0 ? { ...model, presets: [OPEN_IN_RECAST] } : model;
}
```

`src/companion/groups.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { groupByKind } from "./groups";
import type { FileInfo, Kind } from "../shared/types";

const f = (name: string, kind: Kind): FileInfo => ({
  path: `C:\\x\\${name}`, name, kind, ext: "", format: "", size: 1, duration_s: null, has_audio: null, pages: null,
});

describe("groupByKind", () => {
  it("groups in a fixed kind order, keeping drop order inside a group", () => {
    const g = groupByKind([f("b.png", "image"), f("r.pdf", "pdf"), f("a.png", "image"), f("v.mov", "video")]);
    expect(g.map((x) => x.kind)).toEqual(["video", "image", "pdf"]);
    expect(g[1]).toMatchObject({ label: "2 images", names: ["b.png", "a.png"], paths: ["C:\\x\\b.png", "C:\\x\\a.png"] });
    expect(g[2].label).toBe("1 PDF");
  });
  it("returns nothing for no files", () => {
    expect(groupByKind([])).toEqual([]);
  });
});
```

`src/companion/groups.ts`:

```ts
/** Mixed drops split into one group per kind (spec §5.6). */
import type { FileInfo, Kind } from "../shared/types";

export interface Group {
  kind: Kind;
  label: string;
  paths: string[];
  names: string[];
}

const ORDER: Kind[] = ["video", "audio", "image", "pdf", "text", "unknown"];
const NOUN: Record<Kind, [string, string]> = {
  video: ["video", "videos"],
  audio: ["audio file", "audio files"],
  image: ["image", "images"],
  pdf: ["PDF", "PDFs"],
  text: ["document", "documents"],
  unknown: ["other file", "other files"],
};

export function groupByKind(files: FileInfo[]): Group[] {
  return ORDER.flatMap((kind) => {
    const fs = files.filter((x) => x.kind === kind);
    if (fs.length === 0) return [];
    const [one, many] = NOUN[kind];
    return [{ kind, label: `${fs.length} ${fs.length === 1 ? one : many}`, paths: fs.map((x) => x.path), names: fs.map((x) => x.name) }];
  });
}
```

Run `npm test`: the new tests pass.

- [ ] **Step 2: The Rust command**

In `ipc.rs`:

```rust
#[derive(Serialize, Clone)]
struct OpenFiles {
    paths: Vec<String>,
}

/// The pad's "Open in Recast" (spec §5.6): Activity lists these files grouped by kind.
#[tauri::command]
pub async fn open_in_recast(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    crate::tray::show_companion(&app);
    app.emit_to("companion", "companion://open-files", OpenFiles { paths })
        .map_err(|e| e.to_string())
}
```

Register it in `generate_handler!`, add it to `build.rs`, and add `"allow-open-in-recast"` to `capabilities/pad.json`. In `ipc.ts`, add `openInRecast: (paths: string[]) => invoke<void>("open_in_recast", { paths }),` and `export const onOpenFiles = (cb: (paths: string[]) => void) => listen<{ paths: string[] }>("companion://open-files", (e) => cb(e.payload.paths));`.

- [ ] **Step 3: The pad**
- `PadApp.tsx` `begin`: after the `Promise.all`, set `model = withOpenInRecast(model);` (import it).
- `usePad.ts` starting effect: at the top, after the `phase.name !== "starting"` guard, add:

```ts
    if (phase.presetId === OPEN_IN_RECAST_ID) {
      ipc.openInRecast(paths.current).catch((e) => console.warn("open in Recast failed", e));
      dispatch({ type: "dismiss" });
      return;
    }
```

  (import `OPEN_IN_RECAST_ID` from `./openInRecast`). `reduce` handles `dismiss` in any open phase with `shut`, and the pad closes as after Esc.

- [ ] **Step 4: The companion**
- `InlineRing.tsx`: add the prop `onStarted?: () => void` and:

```tsx
  const startedOnce = useRef(false);
  useEffect(() => {
    if (pad.state.phase.name === "starting" && !startedOnce.current) {
      startedOnce.current = true;
      onStarted?.();
    }
  }, [pad.state.phase.name, onStarted]);
```

- `App.tsx`: hold `const [incoming, setIncoming] = useState<{ paths: string[]; n: number } | null>(null);`. Add an effect that subscribes to `onOpenFiles((paths) => { setSection("activity"); setIncoming((x) => ({ paths, n: (x?.n ?? 0) + 1 })); })` and unlistens in cleanup. Pass `incoming` to `<Activity … incoming={incoming} />`.
- `Activity.tsx`:
  - state: `const [groups, setGroups] = useState<Group[]>([]);`. `Dropped` gains `kind?: Kind`.
  - one function used by drops and by `incoming`:

```tsx
  const receive = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    try {
      setError(null);
      const pad = await ipc.inspect(paths);
      if (pad.open_in_recast) {
        setDropped(null);
        setGroups(groupByKind(pad.files));
      } else {
        setGroups([]);
        setDropped({ paths, pad });
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);
```

  - The router's drop branch becomes `setHovering(false); void receive(ev.paths);`, and the effect depends on `[receive]`.
  - `useEffect(() => { if (incoming) void receive(incoming.paths); }, [incoming, receive]);`.
  - Opening a group:

```tsx
  const openGroup = async (g: Group) => {
    try {
      setDropped({ paths: g.paths, pad: await ipc.inspect(g.paths), kind: g.kind });
    } catch (e) {
      setError(String(e));
    }
  };
```

  - Render it right after the error line, before the `InlineRing`:

```tsx
      {groups.length > 0 && (
        <div className="groups" aria-label="Dropped files by kind">
          <p>These files are different kinds. Convert each group on its own.</p>
          <ul>
            {groups.map((g) => (
              <li key={g.kind}>
                <span>
                  <strong>{g.label}</strong>{" "}
                  <span className="muted">{g.names.slice(0, 3).join(", ")}{g.names.length > 3 ? "…" : ""}</span>
                </span>
                <button disabled={g.kind === "unknown"} title={g.kind === "unknown" ? "Recast has no conversions for these" : undefined}
                  onClick={() => openGroup(g)}>
                  Choose conversion
                </button>
              </li>
            ))}
          </ul>
          <button className="ghost" onClick={() => setGroups([])}>Clear</button>
        </div>
      )}
```

  - The `InlineRing` gets `onStarted={() => { const k = dropped.kind; if (k) setGroups((gs) => gs.filter((x) => x.kind !== k)); }}`.
- `companion.css`: `.groups` gets the card look already used by `.job` rows (reuse its border and background variables). Rows are flex with `justify-content: space-between`, and `.groups ul` has no bullets.

- [ ] **Step 5: Verify and commit**

Run `npm test && npm run build`, then `cd src-tauri && cargo test -j 2 -- ipc:: && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check`.

In the running app:
1. Drop a PNG and a PDF on Activity together. The groups panel shows "1 image" and "1 PDF". **Choose conversion** on the image opens the ring with image presets, and running one removes that group.
2. `recast.exe --pad <png> <pdf>` gives a pad with one **Open** segment. Clicking it closes the pad and shows the companion with the same groups.

```bash
git add -A src src-tauri/src src-tauri/build.rs src-tauri/capabilities/pad.json
git commit -m "feat: Open in Recast for mixed kinds (pad segment and Activity groups)"
```

---

### Task 11: Trigger settings and the first-run welcome

**Files:**
- Create: `src/companion/Welcome.tsx`
- Modify: `src/companion/Settings.tsx`, `src/companion/App.tsx`, `src/companion/companion.css`

- [ ] **Step 1: Lift settings into `App`**

`App` holds `const [settings, setSettings] = useState<Settings | null>(null)` and `const [settingsError, setSettingsError] = useState<string | null>(null)`. It loads them once with `ipc.getSettings()` and defines:

```tsx
  const save = useCallback(
    (patch: DeepPartial<Settings>) =>
      ipc.setSettings(patch).then(setSettings, (e) => setSettingsError(String(e))),
    [],
  );
```

`SettingsView` becomes `SettingsView({ s, error, save }: { s: Settings | null; error: string | null; save: (p: DeepPartial<Settings>) => Promise<unknown> })`. Delete its own state and `useEffect`, and keep all the fields.

- [ ] **Step 2: The new sections** (in `Settings.tsx`; export `TriggerFields` for the welcome)

```tsx
export function TriggerFields({ s, save }: { s: Settings; save: (p: DeepPartial<Settings>) => Promise<unknown> }) {
  const t = s.triggers;
  return (
    <>
      <Toggle label="Edge handle" hint="A thin tab on the screen edge. Drag a file onto it to open the pad."
        checked={t.edge_handle} onChange={(v) => save({ triggers: { edge_handle: v } })} />
      {t.edge_handle && (
        <>
          <Field label="Edge">
            <select value={t.edge} onChange={(e) => save({ triggers: { edge: e.target.value as Settings["triggers"]["edge"] } })}>
              <option value="left">Left</option>
              <option value="right">Right</option>
              <option value="top">Top</option>
              <option value="bottom">Bottom</option>
            </select>
          </Field>
          <Field label={`Position along the edge (${Math.round(t.edge_pos * 100)}%)`}>
            <input type="range" min={0} max={100} step={5} value={Math.round(t.edge_pos * 100)}
              onChange={(e) => save({ triggers: { edge_pos: Number(e.target.value) / 100 } })} />
          </Field>
        </>
      )}
      <Toggle label="Desk pad" hint="A round pad on your desktop that you can drop files on. Drag it to move it."
        checked={t.desk_pad} onChange={(v) => save({ triggers: { desk_pad: v } })} />
      <Toggle label="Shake to open" hint="Shake a file quickly left and right while you drag it. Off while a fullscreen app is in front."
        checked={t.shake} onChange={(v) => save({ triggers: { shake: v } })} />
    </>
  );
}

function Toggle({ label, hint, checked, onChange }: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="field toggle">
      <span>
        {label}
        {hint && <small className="muted">{hint}</small>}
      </span>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}
```

In `SettingsView`, before "Output", insert `<h2>Opening the pad</h2><TriggerFields s={s} save={save} />`. After "Jobs", add:

```tsx
      <h2>Windows</h2>
      <Toggle label="“Open with Recast” in Explorer" hint="Right-click files, then Show more options."
        checked={s.shell.explorer_entry} onChange={(v) => save({ shell: { explorer_entry: v } })} />
      <Toggle label="Start with Windows" hint="Recast waits in the tray, ready for a drop."
        checked={s.app.start_with_windows} onChange={(v) => save({ app: { start_with_windows: v } })} />
      <p className="muted">Drags from apps running as administrator can't reach Recast; Windows blocks them.</p>
```

- [ ] **Step 3: `Welcome.tsx`**

```tsx
/** First run (spec §4.6): the edge handle, the tray icon, the trigger toggles. */
import { useEffect, useState } from "react";
import { ipc } from "../shared/ipc";
import type { DeepPartial, Settings } from "../shared/types";
import { TriggerFields } from "./Settings";

const EDGE = { left: "left", right: "right", top: "top", bottom: "bottom" } as const;

export function Welcome({ s, save, onDone }: { s: Settings; save: (p: DeepPartial<Settings>) => Promise<unknown>; onDone: () => void }) {
  const [step, setStep] = useState(0);
  useEffect(() => {
    ipc.highlightHandle(step === 0).catch(() => {});
  }, [step]);
  useEffect(() => () => void ipc.highlightHandle(false).catch(() => {}), []);
  const finish = () => {
    void save({ app: { first_run_done: true } }).finally(onDone);
  };
  const steps = [
    {
      title: "Drag a file here",
      body: s.triggers.edge_handle
        ? `The glowing tab on the ${EDGE[s.triggers.edge]} edge of your screen is the edge handle. Drag any file onto it and the pad opens beside it. Release on a conversion and the new file appears next to the original.`
        : "The edge handle is off. You can turn it on in the last step.",
    },
    {
      title: "Recast lives in the tray",
      body: "Recast keeps running in the notification area by the clock. Click its icon to open this window. Right-click it for Convert files… and Quit.",
    },
    { title: "Choose how the pad opens", body: null },
  ];
  const cur = steps[step];
  return (
    <div className="welcome-overlay" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <div className="welcome">
        <p className="muted">Welcome to Recast · {step + 1} of 3</p>
        <h2 id="welcome-title">{cur.title}</h2>
        {cur.body ? <p>{cur.body}</p> : <TriggerFields s={s} save={save} />}
        <div className="welcome-actions">
          <button className="ghost" onClick={finish}>Skip</button>
          {step < 2 ? <button onClick={() => setStep(step + 1)} autoFocus>Next</button> : <button onClick={finish} autoFocus>Done</button>}
        </div>
      </div>
    </div>
  );
}
```

In `App.tsx`, `const [welcomeClosed, setWelcomeClosed] = useState(false);`, and render `{settings && !settings.app.first_run_done && !welcomeClosed && <Welcome s={settings} save={save} onDone={() => setWelcomeClosed(true)} />}` as the last child of `.shell`.

`companion.css`:
- `.welcome-overlay`: fixed, inset 0, a dark translucent scrim, grid centring, z-index above `.ring-overlay`.
- `.welcome`: a card, max-width 460 px, padding 24 px, using the companion's existing surface variables.
- `.welcome-actions`: flex, `justify-content: flex-end`, gap 8 px.
- `.field.toggle small`: display block, font-size 12 px.

- [ ] **Step 4: Verify and commit**

Run `npm test && npm run build`.

In the running app, start with a fresh settings file (rename `%APPDATA%\Recast\settings.json` aside, and restore it after).
1. The welcome appears and the handle pulses on step 1. Step 3's toggles work: turning the edge handle off hides the handle at once.
2. Done closes the welcome, and it doesn't come back on restart.
3. In Settings, the Edge select and the Position slider move the handle live.

```bash
git add -A src/companion
git commit -m "feat(companion): trigger, Explorer and startup settings; first-run welcome"
```

**→ Opus checkpoint review 2 (Tasks 8–11).**

---

### Task 12: Uninstall hook, QA checklist, docs and full verification

**Files:**
- Create: `src-tauri/windows/hooks.nsh`, `docs/qa-checklist.md`
- Modify: `src-tauri/tauri.conf.json`, `docs/superpowers/specs/2026-11-18-recast-design.md`, `README.md`, `docs/superpowers/handoff-plan-3.md` (rename to `handoff-plan-4.md` with new content)

- [ ] **Step 1: The NSIS hook**

`src-tauri/windows/hooks.nsh`:

```nsis
; Recast's uninstall cleanup (spec §6.12). Tauri's template already removes the Run value
; named after the product ("Recast"); this removes the Explorer verb and Recast's data.
; Nothing here runs during an update (/UPDATE), which reinstalls over the old version.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegKey HKCU "Software\Classes\*\shell\Recast"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Recast"
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\Recast"
    ${If} $DeleteAppDataCheckboxState = 1
      RMDir /r "$APPDATA\Recast"
    ${EndIf}
  ${EndIf}
!macroend
```

In `tauri.conf.json`, `bundle.windows.nsis` becomes `{ "installMode": "currentUser", "installerHooks": "./windows/hooks.nsh" }`.

- [ ] **Step 2: Build the installer and check the generated script**

Run: `npm run tauri build`. It takes a while, and it's the only build running.
Expected: `src-tauri/target/release/bundle/nsis/Recast_0.1.0_x64-setup.exe`.

Then run `grep -n "shell\\\\Recast\|RMDir /r \"\$LOCALAPPDATA\\\\Recast\"" src-tauri/target/release/nsis/x64/installer.nsi`. It should show the hook's lines inlined inside the uninstall section.

Install and uninstall need a human; they're in the checklist.

- [ ] **Step 3: `docs/qa-checklist.md`**

Write the spec §9 manual checklist as checkbox sections:
- **Triggers:** edge handle drag, drop on the handle, auto-hide over fullscreen video, edge and position settings, desk pad drag and drop, position remembered, shake from Explorer, shake false positives (text selection, painting, a fullscreen game, Aero Shake), the 1.5 s auto-close after a shake with no drop.
- **Explorer:** "Open with Recast" with 1, 20 and 100 files, one pad, the pad takes keyboard focus; entry removed when the setting is off.
- **Startup:** start with Windows, starts hidden in the tray; first-run welcome once.
- **Pad:** 100%/150%/200% DPI, two monitors, reduced motion and reduced transparency, keyboard only including More/Back, light and dark wallpapers, no ghost ring in the glass, `?perf` ≤ 4 ms, a real Explorer drag onto the pad.
- **Notifications:** Show in folder, Details, Save to fallback folder (a read-only destination folder), engine Retry (disconnect the network during a first download).
- **Mixed kinds:** pad Open segment, Activity groups.
- **Tools:** reorder by mouse and keyboard, Merge, Split with valid and invalid ranges.
- **Tray:** Convert files…, glyph follows the taskbar theme.
- **Uninstall:** no verb, no Run value, no `%LOCALAPPDATA%\Recast`; `%APPDATA%\Recast` only with "Delete app data".
- **Elevated:** a drag from an elevated app is ignored without errors.
- **Plan 1 manual checks:** the 8 items from the Plan 1 handoff.

Each item says what to do and what should happen in one or two lines.

- [ ] **Step 4: Fold the deviations into the spec**
- Update §4.1, §4.5 (tray menu: no Drop pad; Settings sections), §4.6, §5.6, §6.7, §6.10, §6.11 (new commands `pad_drag_entered`, `open_in_recast`, `highlight_handle`; events `companion://navigate`, `companion://open-files`, `widget://hover`, `widget://highlight`; capability `widgets.json`; `debug_open_pad` removed), §6.12, §6.13 (new files), §7 (engine Retry exists now), §9 and §10 (`triggers.desk_x/desk_y`) with deviations 1–11.
- Update the status line at the top: Plans 1–3 implemented.
- README: how the pad opens, and the `RECAST_DEV_INTEGRATION` note.
- `git mv docs/superpowers/handoff-plan-3.md docs/superpowers/handoff-plan-4.md` and rewrite it for whoever does the release and manual QA. Cover state, test counts, what is left (the QA checklist, the owner decisions, unfixed findings) and the practical gotchas, keeping the useful ones from the old handoff.

- [ ] **Step 5: Full verification**

```bash
cd src-tauri && cargo test -j 2 && cargo clippy -j 2 --all-targets -- -D warnings && cargo fmt --check
cargo test -j 2 --features engines --test engines_integration -- --test-threads=1
cd .. && npm test && npm run build
```

Expected: everything passes. Record the counts (Rust unit, Vitest, engines integration) for the handoff.

Then run the app (`npm run tauri dev`) and walk through every "in the running app" check from Tasks 3, 7, 8, 10 and 11 once more on the final code. Note anything that needs a human.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/windows src-tauri/tauri.conf.json docs README.md
git commit -m "docs: QA checklist, Plan 3 decisions in the spec, uninstall hook, handoff"
```

**→ Final Opus review.**

---

## Self-review notes (writer)

- **Spec coverage:**

  | Spec item | Task |
  |---|---|
  | §4.1 rows | 7 (handle, desk pad, shake), 3 (Explorer) |
  | §4.5 Settings | 11 |
  | §4.6 | 11 |
  | Notification buttons | 8 |
  | Tray glyphs | 9 |
  | §5.6 | 10 |
  | §6.7 | 4, 5, 6, 7 |
  | §6.10 | 2, 3 |
  | §6.12 | 12 (the Run value is also removed by Tauri's template) |
  | §9 checklist | 12 |
  | Removing the stand-ins | 7 |
- **Types used across tasks:**
  - `PadOpen.probe` (4) is used in PadApp (4).
  - `pad::open_at`, `is_open`, `is_open_with`, `open_probe` and `release_grace` (4) are used by triggers (7).
  - `AppState::update_settings` and `settings_now` (3) are used by ipc (3) and triggers (7).
  - `Target` (8, `jobs::notify`) is re-exported by `toast`.
  - `platform::MouseHook`, `MouseInput`, `foreground_info` and `watch_foreground` (6) are used in triggers (7).
  - `taskbar_is_light` (1) is used in tray and triggers (9).
- **Known risks for the reviewers:**
  - The 6 px window width may be refused by the OS. Task 7 Step 6 has a fallback.
  - Toast buttons depend on the plugin's Windows tracking; a click is reported only while the toast is on screen.
  - `RegDeleteTreeW` deleting the named key itself is checked by Task 1's test.
  - The shake probe relies on OLE re-targeting a window shown under the cursor mid-drag. Plan 2 showed this works for the drag-mode pad.
