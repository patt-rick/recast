# Handoff: Recast — start Plan 3 (triggers, shell, first run, installer hooks)

Paste this into a new session, or say "read docs/superpowers/handoff-plan-3.md and continue".

## Where things stand (2026-11-19)

- **Repo:** `C:\Users\Patrick Ackom\Desktop\repos\recast`
- **Branch:** `plan-2-pad`, branched from `master` (which has Plan 1 merged). Nothing pushed. Not merged yet: ask the owner how to land it (merge into `master` or open a PR) before starting Plan 3 work, then branch Plan 3 from wherever Plan 2 lands.
- **Plan 2 is done:** the Liquid Glass drop pad, backdrop capture, thumbnails and the Tools page.
  - The ring appears inline in Activity (solid look) and in its own transparent, topmost, capture-excluded pad window (glass over the captured backdrop).
  - Tools has Merge PDFs, Images → PDF (dnd-kit ordering with pointer and keyboard) and Split PDF by page ranges.
  - Per-job options (`ranges`) are stored in history schema 2, so Retry keeps them.
  - The pad opens from the tray: **Convert files…** (picker, then click mode at the cursor) and **Drop pad** (drag mode, no files). Both are Plan 2 stand-ins.
- **Tests, all passing:**
  - 175 Rust unit tests
  - 81 Vitest tests
  - 11 real-engine integration tests (including `pdf-extract` and thumbnails)
  - clippy and rustfmt clean
- **Release build:** the NSIS installer builds (`Recast_0.1.0_x64-setup.exe`, 6.1 MB).
- **Verified in the running app (2026-11-19)**, through WebView2 CDP and Cua desktop control:
  - the inline ring: light solid look, hover pill, More/Back paging, keyboard focus, Esc
  - the pad window in click mode: Liquid Glass over the captured backdrop, per-segment light/dark adaptation, hover lift, tint and pill, Esc hides the window
  - the pad absent from the desktop capture
  - drag mode: the waiting state "Drop files here" shown without focus, and hidden on dismiss
  - PNG → WebP from the pad, including the ImageMagick download label and done → close
  - Tools: Merge with a pointer drag reorder (order kept), and Split with invalid and valid ranges
  - `?perf`: mean draw about 0.4 ms at about 60 fps (budget 4 ms)

## Read first

1. `docs/superpowers/specs/2026-11-18-recast-design.md`: the spec, updated with every Plan 1 and Plan 2 decision. Plan 3 covers **§4.1** (triggers), **§4.6** (first run), **§6.7** (edge handle, desk pad, shake, auto-hide), **§6.10** (Explorer entry, single instance, focus), **§6.12** (installer hooks) and `docs/qa-checklist.md` (§9).
2. `docs/superpowers/plans/2026-11-18-recast-plan-2-pad.md`: the top lists Plan 2's scope, the 12 deliberate deviations and the binding review amendments A1–A10. You don't need the task bodies.
3. `src-tauri/src/pad.rs`: the pad window controller your triggers call. Read its generation, show and close logic before writing a trigger.
4. `docs/mockups/pad-liquid-glass.html` only if you touch the visuals.

## Plan 3 scope (the spec's deferred list)

- **Edge handle** (§4.1, §6.7): a 6 × 80 px tab on a screen edge, auto-hides while a fullscreen app is in front. A drag entering it opens the pad; a drop on it opens click mode.
- **Desk pad** (§4.1): a 96 px round glass widget at desktop level.
- **Shake while dragging** (§4.1, §6.7): `WH_MOUSE_LL` hook on its own thread, detector as a pure function with recorded sample streams.
- **Explorer entry and argv forwarding** (§6.10): `HKCU\Software\Classes\*\shell\Recast`, `MultiSelectModel=Player`, single-instance forwarding with a sliding 300 ms debounce, `AllowSetForegroundWindow`.
- **First run** (§4.6), **start with Windows**, **notification buttons** (Show in folder, Details, Save to fallback folder, engine-install Retry), **tray glyphs** (`assets/icon/tray-*.svg`).
- **NSIS uninstall hook** (§6.12) and **`docs/qa-checklist.md`** (§9).
- **Trigger settings UI** (§4.5 Settings → Triggers).

**How triggers reach the pad:** call `pad::open(app, paths, PadMode::Drag)` for a mid-drag trigger (shown without focus, cursor feed on) or `PadMode::Click` for Explorer and handle drops (shown and focused once the page has drawn). `open` is safe to call from any thread. When real triggers exist, **remove the tray "Drop pad" item and the debug-only `debug_open_pad` command**. "Convert files…" can stay.

## How to work (owner's standing rules: follow exactly)

- **All reviews use an Opus subagent (`model: opus`)**, never another model, unless the owner says otherwise.
  - Review the plan adversarially and read-only before building, and again before reporting done.
  - Fold the findings in. **Report every finding left unfixed**; never quietly drop minors.
  - Lint-only fixes need no review.
- **Work end to end without check-ins** on an approved task; report once at the end.
- **Status questions about background agents start from a fresh `ListAgents` call.**
- **Process that worked for Plans 1 and 2:**
  1. `superpowers:writing-plans`: save as `docs/superpowers/plans/<date>-recast-plan-3-triggers.md`.
  2. Opus plan review, with the findings written into the plan as binding amendments.
  3. Subagent-driven execution: Sonnet implementers on sequential task groups.
  4. Opus checkpoint reviews every few groups.
  5. Fix rounds, then a final Opus review.
- **Commits:** branch first, never commit to `master`/`main` directly, never push without being asked. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Practical gotchas

- **Windows 10, Git Bash.** Rust 1.96 MSVC, Node 24. Cargo runs from `src-tauri/`.
- **Rust test filters go after `--`**, as in `cargo test -j 2 -- jobs::`. A filter before `--` skips building `examples/fake_engine.rs`, and every runner and job test then fails with "fake_engine.exe is missing".
- **The machine runs low on memory.** Run one cargo build at a time, use `-j 2`, and don't run builds in parallel agents.
- **Write and edit files with the Write/Edit tools, not shell heredocs.** Subagents have collapsed `\\` to `\` through heredocs. Re-read backslash-heavy strings (Windows paths, regexes) after writing them.
- **Real-engine tests:** `cargo test -j 2 --features engines --test engines_integration -- --test-threads=1`. The cache is `src-tauri/target/engine-cache/` (downloads plus installed engines).
- **A running `recast.exe` locks `target/debug/recast.exe`.** Either stop only the process you started, or build with `CARGO_TARGET_DIR="$LOCALAPPDATA/Temp/<name>"`.
- **Insta snapshots:** after a preset change, run `INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test -j 2 -- presets::catalog_tests`, then a plain run, then actually read the changed `.snap` files.
- **Tauri dev cold start:** Vite can take about 20 s to serve the first module, so a blank window at first is normal.
- **Front-end checks:** `npm test` (Vitest) and `npm run build` (runs `tsc --noEmit`).
- **Dev testing tips (Plan 2):**
  - Run with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npm run tauri dev`, and drive the pages over CDP.
  - Open the pad from the companion page's console with `window.__TAURI_INTERNALS__.invoke('debug_open_pad',{paths:[...],drag:false})` (debug builds only; `drag:true` for drag mode).
  - Git Bash converts a lone `/` argument into a path, so quote or escape arguments that start with a slash.
  - Tray menu clicks through desktop automation are unreliable because of Windows' foreground lock. Use `debug_open_pad` instead.
- **IPC surface** to build on: `src-tauri/src/ipc.rs` ↔ `src/shared/ipc.ts` + `types.ts`. `platform/win.rs` is the only place OS APIs may be called.

## Still open (tell the owner; not blockers for Plan 3)

- **Plan 1's 8 manual checks still need a human** (Plan 1 Task 21, Step 3):
  1. A real `.mov` drop through download, convert, notification and Show in folder.
  2. Three PNGs giving one combined notification.
  3. A mixed-kinds drop.
  4. Cancel leaving no `.recast-*` folder.
  5. Library verify, remove and re-download.
  6. A settings change persisting.
  7. Tray hide and reopen.
  8. Quitting mid-conversion, then relaunching.
- **Not exercised by automation in Plan 2:**
  - a real mid-drag OLE drag onto the pad (the `Over` and `Drop` targeting from Explorer)
  - 100%, 150% and 200% DPI, and multi-monitor placement
  - a run over a light wallpaper
- **Two decisions for the owner** from Plan 1, already in the spec: Pandoc may fetch remote images a converted document links to (§8), and MOV output re-encodes to H.264/AAC.

## Unfixed review findings

Known and left as they are in Plan 2. Fix the ones that matter as Plan 3 touches the code.

- **`CAPTUREBLT` isn't used** for the backdrop capture, so layered windows of other apps are missing from the image. It avoids cursor flicker.
- **The cursor poll** makes about 3 main-thread round trips per 16 ms.
- **A pad page reload** (WebView2 crash or HMR) leaves `ready` true, so opens sent before the next `pad_ready` are lost.
- **The "Convert files…" dialog** has no owner window.
- **Thumbnail size is read, then the file is read separately** (a TOCTOU on the 25 MB cap).
- **Paging only fades the new symbols in**; the glass doesn't cross-fade.
- **An all-skipped batch shows ✓** in the ring.
- **One failure in a batch** shows only that job's message.
- **JS `\s` and Rust `is_whitespace` differ** for U+FEFF and U+0085 in page ranges.

## First steps for the new session

1. `git status` and `git log --oneline -3` on `plan-2-pad` to confirm a clean tree.
2. Ask the owner how Plan 2 should land (merge or PR), and whether they've done Plan 1's manual checks. Then branch for Plan 3.
3. Write Plan 3 with `superpowers:writing-plans`, against the real code. Read `src-tauri/src/pad.rs`, `lib.rs`, `tray.rs`, `platform/win.rs`, `settings.rs` and `src/companion/Settings.tsx` first.
4. Opus plan review, then build.
