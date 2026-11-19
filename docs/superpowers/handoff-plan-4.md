# Handoff: Recast — release and manual QA (after Plan 3)

Paste this into a new session, or say "read docs/superpowers/handoff-plan-4.md and continue".

## Where things stand (2026-11-19)

- **Repo:** `C:\Users\Patrick Ackom\Desktop\repos\recast`
- **Branch:** `plan-3-triggers`, branched from `plan-2-pad`. Plan 2 is already merged into `master` (fast-forward), so `git log master..plan-3-triggers` holds only Plan 3. Plan 3 lands on `master` the same way (fast-forward) when the owner says so. Nothing pushed.
- **Plan 3 is done:**
  - Triggers: the edge handle, the desk pad and shake-while-dragging (with an invisible probe confirmed by a real file drag), plus auto-hide over fullscreen.
  - Shell: `recast.exe --pad <files>` with single-instance forwarding and a debounce, the Explorer verb, start with Windows (`--autostart`), HKCU registry sync.
  - Notification buttons, tray glyphs that follow the taskbar theme, Open in Recast for mixed kinds, Settings → Opening the pad, the first-run welcome.
  - The NSIS uninstall hook, `docs/qa-checklist.md`, and the spec updated to match (§6.14 lists the deviations).
- **Tests, all passing on the final code:**
  - 222 Rust unit tests run by `cargo test -j 2`, plus 2 ignored ones (see below)
  - 106 Vitest tests (`npm test`)
  - 11 real-engine integration tests
  - The 2 ignored tests in `platform::win` install a real system-wide `WH_MOUSE_LL` hook, so they need a desktop session: `cargo test -j 2 --lib -- --ignored the_mouse_hook_starts_and_stops_cleanly dropping_an_old_hook_does_not_cut_off_a_newer_one --test-threads=1`. Both passed on 2026-11-19.
  - clippy (`-D warnings`) and rustfmt clean; `npm run build` (tsc) clean
- **Release build:** `npm run tauri build` makes `src-tauri/target/release/bundle/nsis/Recast_0.1.0_x64-setup.exe`, 6,460,018 bytes (6.16 MiB). `src-tauri/target/release/nsis/x64/installer.nsi` includes `windows/hooks.nsh` and runs `NSIS_HOOK_POSTUNINSTALL` inside `Section Uninstall`.
- **Checked in a live dev run (2026-11-19, 150% primary plus a 100% second monitor), by window dump and CDP:** the handle window is 6 × 80 logical px (9 × 120 physical, no 198 px caption minimum) and the desk pad 96 × 96; `recast.exe --pad` forwarding opens one click-mode pad on the monitor of the cursor (both monitors, repeatedly); a mixed `--pad` (png + txt) shows the Open segment and pill, and activating it lists "1 image" and "1 document" groups with Choose conversion in Activity; Settings changes move the handle between edges, hide and show it, and show and hide the desk pad; `highlight_handle` grows the handle to 180 × 80 and back.
- **One dev-run oddity, not reproduced:** the first three `--pad` forwards after a cold `npm run tauri dev` start logged `pad: open` but the pad window never became visible, and its page was empty; every later forward (a dozen, on both monitors) worked. It looks like the pad page had not finished loading or registering `pad_ready` after Vite's cold start (the reload bug fixed in the final review round, where a reloading pad page stayed marked ready, is a likely cause). Worth a look if it shows up in a release build.
- **Unverified on real hardware** (the automation had no hands): everything in `docs/qa-checklist.md`. The most important is the shake probe: a real Explorer drag, shaken, must log `pad: drag entered` and show the pad. If it doesn't, give `.pad-body` a `rgba(0,0,0,0.004)` background while probing.

## Read first

1. `docs/qa-checklist.md`: the manual run, before each release.
2. `docs/superpowers/specs/2026-11-18-recast-design.md`: §4.1, §6.7, §6.10, §6.12 and §6.14 describe what Plan 3 built and why it differs from the first draft.
3. `docs/superpowers/plans/2026-11-19-recast-plan-3-triggers.md`: the top lists the 14 deviations and the binding review amendments B1–B15.

## What is left

1. **Run `docs/qa-checklist.md`** on a release install (it needs a human: real drags, shaking, Explorer, taskbar changes, DPI and monitor changes, notification clicks, install, upgrade and uninstall).
2. **Owner decisions** (all already in the spec):
   - **Pandoc and remote images:** Pandoc may fetch remote images that a converted document links to (§8). Keep, or block it?
   - **MOV output** re-encodes to H.264/AAC instead of remuxing (§5.1). Keep?
   - **Uninstall and upgrade data policy (changed spec §6.12):** Tauri's reinstall page runs the old uninstaller without `/UPDATE`, so the first draft ("always delete `%LOCALAPPDATA%\Recast`") would have wiped all engines (about 250 MB) and the history on every manual upgrade. The hook now always removes the verb, the Run value, `logs` and `tmp`, and removes `%LOCALAPPDATA%\Recast` (engines, history) and `%APPDATA%\Recast` (settings) only when "Delete app data" is ticked. The cost: a plain uninstall leaves engines and history behind. Confirm this is what you want.
3. **Landing and release:** when the owner says so, fast-forward `master` to `plan-3-triggers` (as Plan 2 was), then tag a release. The installer is unsigned (SmartScreen warns; §11).
4. **Unfixed earlier review findings** (checked against the code on 2026-11-19; Plan 3 didn't touch any of them):
   - `CAPTUREBLT` isn't used for the backdrop capture, so layered windows of other apps are missing from the image. It avoids cursor flicker.
   - The cursor poll makes several main-thread round trips per 16 ms (it now also samples the mouse buttons).
   - `inspect` probes files sequentially (a 100-video Explorer selection keeps the click-mode pad hidden for seconds).
   - The "Convert files…" dialog has no owner window.
   - Thumbnail size is read, then the file is read separately (a TOCTOU on the 25 MB cap).
   - Paging only fades the new symbols in; the glass doesn't cross-fade.
   - An all-skipped batch shows a tick in the ring.
   - One failure in a batch shows only that job's message.
   - JS `\s` and Rust `is_whitespace` differ for U+FEFF and U+0085 in page ranges.
5. **Plan 2 items still not exercised by automation:** a real mid-drag OLE drag onto the pad from Explorer (targeting and drop), 100/150/200% DPI, multi-monitor placement and a run over a light wallpaper. They are in the checklist.

## How to work (owner's standing rules: follow exactly)

- **All reviews use an Opus subagent (`model: opus`)**, never another model, unless the owner says otherwise. Review plans adversarially and read-only before building, and again before reporting done. Fold the findings in and **report every finding left unfixed**. Lint-only fixes need no review.
- **Work end to end without check-ins** on an approved task; report once at the end.
- **Status questions about background agents start from a fresh `ListAgents` call.**
- **Commits:** branch first, never commit to `master`/`main` directly, never push without being asked. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Practical gotchas

- **Windows 10, Git Bash.** Rust 1.96 MSVC, Node 24. Cargo runs from `src-tauri/`.
- **Rust test filters go after `--`**, as in `cargo test -j 2 -- jobs::`. A filter before `--` skips building `examples/fake_engine.rs`, and every runner and job test then fails with "fake_engine.exe is missing".
- **The machine runs low on memory.** Run one cargo build at a time, use `-j 2`, and don't run builds in parallel agents.
- **Write and edit files with the Write/Edit tools, not shell heredocs.** Subagents have collapsed `\\` to `\` through heredocs. Re-read backslash-heavy strings (Windows paths, regexes) after writing them.
- **Real-engine tests:** `cargo test -j 2 --features engines --test engines_integration -- --test-threads=1`. The cache is `src-tauri/target/engine-cache/` (downloads plus installed engines).
- **A running `recast.exe` locks `target/debug/recast.exe`.** Stop only the process you started, or build with `CARGO_TARGET_DIR="$LOCALAPPDATA/Temp/<name>"`.
- **Cargo rewrites `src-tauri/permissions/autogenerated/*.toml` with different line endings** on every build. Revert that churn (`git checkout -- src-tauri/permissions/autogenerated`) before committing, unless a command was added.
- **Insta snapshots:** after a preset change, run `INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test -j 2 -- presets::catalog_tests`, then a plain run, then actually read the changed `.snap` files.
- **Tauri dev cold start:** Vite can take about 20 s to serve the first module, so a blank window at first is normal.
- **Front-end checks:** `npm test` (Vitest) and `npm run build` (runs `tsc --noEmit`).
- **Opening the pad while developing:** `debug_open_pad` and the tray "Drop pad" item are gone. With the app running, run `src-tauri\target\debug\recast.exe --pad C:\path\a.png C:\path\b.png`: the second process forwards the files to the running instance (after the 300 ms debounce) and exits, and one click-mode pad opens. A repeated launch within 2 s adds to the same pad.
- **`RECAST_DEV_INTEGRATION=1`:** a dev build doesn't write the Explorer verb or the Run value. Set this (exactly `1`) to test them from a dev build, and turn the settings off afterwards, because the entries then point at `target\debug`.
- **Dev builds have no AUMID**, so notification buttons can't be tested there; use a release install.
- **Driving the app:** run with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npm run tauri dev` and drive the pages over CDP (`http://127.0.0.1:9222/json`). The handle and desk pad are separate WebView2 pages (`widgets.html`). To check where widget windows really are, enumerate top-level windows with Win32 (titles `Recast handle`, `Recast desk pad`, `Recast pad`; window class and rect via `EnumWindows` / `GetWindowRect`) rather than trusting settings values. The app also logs the real outer size on first placement (`triggers: handle placed ... window is Some((w, h))`) to `%LOCALAPPDATA%\Recast\logs\recast.log`.
- **Git Bash converts a lone `/` argument into a path**, so quote or escape arguments that start with a slash.
- **Tray menu clicks through desktop automation are unreliable** because of Windows' foreground lock. Use `--pad` forwarding instead.
- **Settings backup:** `%APPDATA%\Recast\settings.json` is the real user file in dev too. Copy it before a live session and put it back afterwards.
- **IPC surface** to build on: `src-tauri/src/ipc.rs` ↔ `src/shared/ipc.ts` + `types.ts`. `platform/win.rs` is the only place OS APIs may be called.
- **The mouse hook callback must stay minimal** (a `try_lock` and a channel send); everything else runs on its worker thread. Don't add logging or locks to it.

## First steps for the new session

1. `git status` and `git log --oneline -5` on `plan-3-triggers` to confirm a clean tree.
2. Ask the owner when to land `plan-3-triggers` on `master` (fast-forward) and for the three owner decisions above.
3. Build the installer (`npm run tauri build`), install it on a test profile, and work through `docs/qa-checklist.md`. Fix what fails with an Opus review before reporting.
