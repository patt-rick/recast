# Handoff: Recast — start Plan 2 (the drop pad)

Paste this into a new session, or say "read docs/superpowers/handoff-plan-2.md and continue".

## Where things stand (2026-11-18)

- **Repo:** `C:\Users\Patrick Ackom\Desktop\repos\recast`
- **Branch:** `plan-1-core`, 37 commits ahead of `master`, last commit `480153a`. Nothing pushed. Not merged yet: ask the owner whether to merge into `main`/`master` or open a PR before starting Plan 2 work (Plan 2 should branch from wherever Plan 1 lands).
- **Plan 1 is done:** the conversion core plus the companion window. You drop files on Activity, pick a preset from a plain list, and get a correctly named output next to the original. Engines (FFmpeg, ImageMagick, Pandoc, qpdf, Ghostscript) download on demand, are SHA-256-pinned and unpacked with the bundled 7-Zip 26.06. Every job has real progress, can be cancelled and never hangs.
- **Every review finding is fixed.** Opus reviewed the plan, three implementation checkpoints, a final pass and two follow-up passes.
- **Tests, all passing:**
  - 149 Rust tests
  - 12 Vitest tests
  - 9 real-engine integration tests: every preset, odd file names, paths over 260 characters
  - clippy and rustfmt clean
- **Installer:** NSIS, 5.9 MB.

## Read first

1. `docs/superpowers/specs/2026-11-18-recast-design.md`: the spec, already updated with every Plan 1 decision. Plan 2 covers **§4.2–4.4** (pad anatomy, states, the Liquid Glass visual spec), **§6.8** (backdrop capture) and the **Tools page** in §4.5 item 2.
2. `docs/superpowers/plans/2026-11-18-recast-plan-1-core.md`: the top of it lists Plan 1's scope, what's deferred to Plans 2 and 3, and the 24 deliberate deviations from the spec. You don't need the task bodies.
3. `docs/mockups/pad-liquid-glass.html`, variant 4 ("Regular, adapting"): the visual source of truth, specifically `regular()` and `rsector()`.

## Plan 2 scope (from Plan 1's "Deferred to Plan 2")

- **The pad window:** 440×440, transparent, frameless, topmost, excluded from capture. Contents:
  - six-segment ring, a centre disc with the thumbnail, name and meta
  - More/Back paging
  - engine download badges
  - hover pills
  - keyboard navigation (←/→/Tab/Enter/Esc)
  - the §4.3 state machine (opening → idle → targeting → running → done/failed)
  - running state as a progress rim, plus a batch indicator
- `src/pad/`: `geometry.ts` (`rsector`), `hitTest.ts`, `states.ts`, and a `glass/` canvas renderer (2D or WebGL, ≤ 4 ms/frame). The blur and saturation are pre-processed into `ImageBitmap`s once per open, so no animated CSS filters.
  - Light/dark adaptation per segment.
  - Reduced-transparency and reduced-motion variants.
- **Backdrop (§6.8):**
  - Rust captures the screen region under the pad through the `platform` module (BitBlt/DXGI); the pad window has `WDA_EXCLUDEFROMCAPTURE`.
  - The image goes to the webview in memory and is never written to disk.
  - Per-segment luminance is computed from it.
  - If capture fails, fall back to the reduced-transparency look.
  - This needs a `capture_backdrop()` IPC command.
- **Thumbnails:** first video frame, image preview, PDF page 1, otherwise a type glyph.
- **GIF warning** when the source is longer than 60 s.
- **Tools page** in the companion:
  - Merge PDFs and Images → PDF, using a reorderable list built with **dnd-kit pointer events**, not HTML5 drag-and-drop. Tauri's `dragDropEnabled` must stay on, and on Windows that disables HTML5 drag-and-drop.
  - Split PDF with a page-range field (`1-3, 5, 8-`).
- **For Plan 2, the pad opens from the companion** (drop on Activity opens the ring inline, §4.5 item 1). Its real triggers come later: edge handle, desk pad, shake, Explorer "Open with Recast", single-instance forwarding, first run, start with Windows, notification buttons and the NSIS uninstall hook are all **Plan 3**.

## How to work (owner's standing rules: follow exactly)

- **All reviews use an Opus subagent (`model: opus`)**, never another model, unless the owner says otherwise.
  - Review the plan adversarially and read-only before building, and again before reporting done.
  - Fold the findings in. **Report every finding left unfixed**; never quietly drop minors.
  - Lint-only fixes need no review.
- **Work end to end without check-ins** on an approved task; report once at the end.
- **Status questions about background agents start from a fresh `ListAgents` call.**
- **Process that worked for Plan 1:**
  1. `superpowers:writing-plans`: save as `docs/superpowers/plans/2026-11-18-recast-plan-2-pad.md` (or today's date).
  2. Opus plan review.
  3. Subagent-driven execution: Sonnet implementers on sequential task groups.
  4. Opus checkpoint reviews every few groups.
  5. Fix rounds, then a final Opus review.
- **Commits:** branch first, never commit to `master`/`main` directly, never push without being asked. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Practical gotchas

- **Windows 10, Git Bash.** Rust 1.96 MSVC, Node 24. Cargo runs from `src-tauri/`.
- **Rust test filters go after `--`**, as in `cargo test -- jobs::`. A filter before `--` skips building `examples/fake_engine.rs`, and every runner and job test then fails with "fake_engine.exe is missing".
- **The machine runs low on memory.** Run one cargo build at a time, use `-j 2`, and don't run builds in parallel agents.
- **Real-engine tests:** `cargo test -j 2 --features engines --test engines_integration -- --test-threads=1`. The cache is `src-tauri/target/engine-cache/` (downloads plus installed engines).
- **A running `recast.exe` locks `target/debug/recast.exe`.** Either stop only the process you started, or build with `CARGO_TARGET_DIR="$LOCALAPPDATA/Temp/<name>"`.
- **Subagents sometimes collapsed `\\` to `\`** when writing files through shell heredocs. Tell them to use the Write/Edit tools and to re-check backslash-heavy strings afterwards.
- **Insta snapshots:** after a preset change, run `INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test -- presets::catalog_tests`, then a plain run, then actually read the changed `.snap` files.
- **Tauri dev cold start:** Vite can take about 20 s to serve the first module, so a blank window at first is normal.
- **IPC surface** to build on: the `src-tauri/src/ipc.rs` ↔ `src/shared/ipc.ts` + `types.ts` pair.
  - `inspect(paths)` returns the `PadModel` (files, presets with engine status, `open_in_recast`), and `start_jobs(presetId, paths)` starts them.
  - Job events arrive on `job://*`; payloads include `batch_id`, `batch_size` and `preset_label`.
  - All commands are async; job and settings state live in `AppState` (`app_state.rs`).
  - `platform/win.rs` is the only place OS APIs may be called.

## Still open from Plan 1 (tell the owner; not blockers for Plan 2)

- **8 manual checks need a human** (Plan 1 Task 21, Step 3):
  1. A real `.mov` drop through download, convert, notification and Show in folder.
  2. Three PNGs giving one combined notification.
  3. A mixed-kinds drop.
  4. Cancel leaving no `.recast-*` folder.
  5. Library verify, remove and re-download.
  6. A settings change persisting.
  7. Tray hide and reopen.
  8. Quitting mid-conversion, then relaunching.
- **Not seen in a running app:** the Library licence viewer and Activity's "Load more" have only been type-checked and built.
- **Two decisions for the owner**, already written into the spec:
  1. Pandoc may fetch remote images that a converted document links to (§8 exception).
  2. MOV output re-encodes to H.264/AAC rather than remuxing.
- **Known limitation:** the plan document still shows old signatures (`list_jobs(page)`, `notice_text(index)`). The spec and code are current, and the plan is a historical record.

## First steps for the new session

1. `git status` and `git log --oneline -3` on `plan-1-core` to confirm a clean tree at `480153a` or later.
2. Ask the owner how Plan 1 should land (merge or PR), and whether they've done the manual checks. Then branch for Plan 2.
3. Write Plan 2 with `superpowers:writing-plans`, against the real code. Read `src/companion/Activity.tsx`, `src/shared/*`, `src-tauri/src/pad_model.rs`, `ipc.rs` and `platform/win.rs` first.
4. Opus plan review, then build.
