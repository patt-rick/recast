# Recast Plan 2 — The Drop Pad, Backdrop, Thumbnails and Tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Plan 1's plain preset list with the Liquid Glass drop pad. The ring appears inline in the companion's Activity view and in its own transparent, topmost pad window. The pad window adapts to a captured backdrop, shows thumbnails, runs jobs with a progress rim, and is fully keyboard-driven. The companion also gains a Tools page (Merge PDFs, Images → PDF, Split PDF by page ranges).

**Architecture:** The ring's logic is pure TypeScript modules in `src/pad/` (geometry, hit-testing, paging, the §4.3 state machine, batch progress, luminance and a per-frame "frame model"), all Vitest-tested. A thin `glass/renderer.ts` draws one frame model onto one `<canvas>` with Canvas 2D: clip paths, transforms and two pre-blurred `ImageBitmap`s, with no animated CSS filters. `Ring.tsx` is the shared presentational component, and `usePad.ts` wires the state machine to IPC. The Rust side adds a `pad.rs` window controller, a `backdrop.rs` capture encoder over new `platform` GDI calls, `thumbs.rs`, page ranges for the new tools-only `pdf-extract` preset, and per-job options stored in history.

**Tech Stack:** Tauri 2.12 (second window, `tauri::ipc::Response` for binary payloads, `tauri-plugin-dialog`), Rust + `windows` 0.62 (GDI `BitBlt`, `SetWindowDisplayAffinity`, registry), React 19 + TypeScript, Canvas 2D, `@dnd-kit/core` + `@dnd-kit/sortable`, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-18-recast-design.md`. Section references (§) point there. The visual source of truth is `docs/mockups/pad-liquid-glass.html`, functions `regular()` and `rsector()`.

---

## Scope of this plan

Plan 2 delivers:
- §4.2 (pad anatomy)
- §4.3 (states, keyboard, running/done/failed)
- §4.4 (visual spec)
- §4.5 item 1 (the ring inline in Activity) and item 2 (Tools)
- §6.8 (backdrop)
- §6.11's `capture_backdrop`
- §6.1's thumbnails
- the GIF > 60 s warning
- the pad window lifecycle part of §6.7: the window is created hidden at startup and reused, shown no-activate mid-drag or focused in click mode, and mid-drag targeting comes from `Over{position}`

**Still Plan 3:** edge handle, desk pad, shake, Explorer entry and argv forwarding, first run, start with Windows, notification buttons, tray glyphs, the NSIS hook, `docs/qa-checklist.md`, and the trigger settings UI.

**How the pad window opens in Plan 2:**
- The tray menu gains **"Convert files…"**. It opens a file picker, then the pad at the cursor in click mode.
- While the pad is open, dragging files onto it targets segments mid-drag. That exercises drag mode, the cursor poll and the leave-to-dismiss rule.
- Plan 3 calls the same `pad::open` from its triggers.

## Deliberate deviations from the spec (fold into the spec in Task 18)

1. **Split PDF semantics.** Each comma-separated range in the page-range field becomes **one PDF**, saved inside the `folder:pages` output folder (named as Plan 1's `pdf-split` folder is, e.g. `report (pages)`): `1-3, 5, 8-` gives `pages-1-3.pdf`, `page-5.pdf` and `pages-8-end.pdf`. An empty field keeps Plan 1's "one PDF per page" (`pdf-split`). This uses a new **tools-only** preset `pdf-extract` with the placeholders `{range}` and `{range_name}`. A pass that uses them runs once per range.
2. **Per-job options.** `start_jobs(preset_id, paths, options?)` takes `JobOptions { ranges }`. Options are stored as JSON in a new `jobs.options` column (history schema 2), so Retry re-runs with the same ranges.
3. **Preset fields** `tools_only` (never offered in the ring), and `warn_over_s` + `warn_text`, the data-driven GIF warning. `PresetView` gains `engine_size` and `warning`.
4. **Luminance is computed in the webview**, from the same captured image, by a pure TS function. Rust only captures. Luminance is Rec. 709 luma on sRGB values (no linearisation), and a value > 0.6 means light glass.
5. **Inner light** is drawn as a clipped 7 px stroke with a canvas `shadowBlur` of 4.4 px (≈ 2 × the 2.2 px SVG std-dev), not an SVG Gaussian blur. Per-frame `ctx.filter` blurs are too slow.
6. **Open animation blur 0 → 5 px** is a cross-fade from the unblurred capture to the pre-blurred bitmap (glass alpha follows the ease). Animating a real blur radius per frame would break the 4 ms budget.
7. **Six fixed slots.** Pages with fewer than six tools show the unused slots as empty, non-targetable glass.
   - Paging: if there are more than six tools, page 1 is 5 tools + More.
   - Each further page is Back + 4 tools + More, or Back + up to 5 tools on the last page.
8. **Hover pill** uses a solid `rgba(30,30,36,.88)` fill. `backdrop-filter` inside a transparent webview can't see the desktop anyway.
9. **Reduced transparency** is read from `HKCU\…\Themes\Personalize\EnableTransparency` (0 means reduced) and passed to the pad. Reduced motion uses the `prefers-reduced-motion` media query, which WebView2 maps to Windows' "Show animations" setting.
10. **The inline ring in Activity** always uses the solid ("reduced transparency") look, because the companion isn't excluded from capture and would capture itself. Mid-drag targeting inside the companion isn't offered: a drop opens the ring, and you then click or use the keyboard (§4.5 item 1).
11. **Thumbnails** come from a separate `thumbnail(path)` command, not from `inspect`, so the ring never waits on ffmpeg:
    - PNG, JPEG, GIF, WebP, BMP, AVIF and ICO up to 25 MB are sent as their own bytes for the webview to decode.
    - HEIC, TIFF and JXL go through ImageMagick, video through ffmpeg, and PDF page 1 through Ghostscript, each only if that engine is installed (never downloaded for a thumbnail).
    - Each has a 5 s cap. ImageMagick and Ghostscript only get paths made of plain ASCII without `%`, `[` or `]`. Anything else gets the type glyph.
    - `thumbnail` only serves paths that a recent `inspect` returned (the last 256), so the webview can't read arbitrary files.
12. **Pad tray entry:** see "How the pad window opens in Plan 2" above.


## Review amendments (Opus plan review, binding: these override the task text below)

**A1. The pad can't be closed for good (H1).** Task 7: in `lib.rs` `on_window_event`, also handle `window.label() == pad::LABEL`:
- call `api.prevent_close()`
- emit `pad://dismiss` to the pad (`window.app_handle().emit_to(pad::LABEL, "pad://dismiss", ())`)
- call `crate::pad::close(window.app_handle())`

Task 8 adds `onPadDismiss(cb)` in `ipc.ts`. Task 15's PadApp listens and dispatches `{ type: "dismiss" }`. Alt+F4 must never destroy the pad window.

**A2. Drag mode is reachable and targets the dragged files (H2, M9).**
- Task 7 adds a second tray item, **"Drop pad"** (id `droppad`). It opens the pad at the cursor in `PadMode::Drag` with **no paths**. Plan 3's triggers replace it.
- Task 15 PadApp rules:
  - `enter` with paths that differ from the session's (or no session/paths yet) calls `begin({ paths, mode: "drag", reduced_transparency: <from the last pad://open> })`, so `Over` targeting applies to the files being dragged. Store `reduced_transparency` from each `pad://open` in a ref.
  - A drag-mode open with empty `paths` shows the pad in a waiting state: disc title "Drop files here", every slot empty. Don't call `inspect([])`. Use `EMPTY` and skip `padRef.current.open` until files arrive. Leave-to-dismiss still applies, through the cursor poll that `pad::open` starts for drag mode (call `track_cursor(app, true)` inside `pad::open` when `mode == Drag`).
  - Blur dismisses **only** in click mode, never while `dragging.current` is true.
  - After any drop on the pad, switch the session's mode to `"click"`, call `getCurrentWindow().setFocus()`, and keep the session. Then a disc drop leaves a pad that Esc and click-outside can still close.
  - A drop of paths different from the session's runs `begin` with mode `"click"` (as the task says).

**A3. Catalog label test (M1).** Task 2, Step 6: in `catalog_tests.rs` `labels()`, filter out `tools_only` presets (`.filter(|p| !p.tools_only)`), so `pdf_ring_order_matches_spec` keeps passing.

**A4. Who shows and hides the pad (M5, low cursor-poll item).** Task 7:
- `PadState` holds one `Mutex<Inner { ready: bool, pending: Option<PadOpen>, generation: u64 }>`.
  - Every `open` increments `generation` and puts it in `PadOpen` (`generation: u64`).
  - Readiness and pending are read and written under that one lock.
- `pad_close(generation)` is a no-op unless `generation` is current. The TS type gets `generation: number`, and PadApp's `finish` sends the session's generation.
- **Click mode** positions the window in `open` but doesn't show it. A new command `pad_show(generation)`, a no-op if stale, shows and focuses it. PadApp calls it once the Ring has drawn its first frame: Ring gets an `onFirstFrame` prop, called after the first `draw`.
- **Drag mode** still shows immediately with `show_no_activate`, because OLE must find the window under the cursor.
- While nothing is shown, a slow `inspect` (ffprobe up to 15 s) leaves no invisible window swallowing clicks.
- The cursor poll uses a generation counter (`AtomicU64`) instead of a bool. `track_cursor(on)` bumps it. Each thread exits when the counter no longer equals the value it started with, and never writes the flag. Then a close followed by a quick reopen can't leave two pollers running or kill the new one.

**A5. Indeterminate rim (M6).** Task 3, in Rust: when `preset.progress == ProgressKind::None`, the `running` events carry `pct: None` (find where `execute`/`run_job` emits `running` with `Some(0.0)`, and emit `None` for those presets). Update any job test that asserts the initial pct. Activity already shows `Working…` for `null`.

**A6. Frame budget (M10).** Task 14, Ring loop:
- Stop requesting animation frames when nothing is moving. Restart the loop when `dirty` is set, from the effect that marks dirty, by holding a `kick()` function in a ref.
- Skip `draw` entirely while `phase.name === "closed"`.
- Task 13's `drawGlass` keeps the per-piece shadows. If Task 18's `?perf` measurement is over **4 ms**, render the shadow and rim layer once into an `OffscreenCanvas` whenever the springs and retract are settled, blit that, and redraw it only while something moves. Record the before and after numbers in the report.

**A7. Bitmap lifetime (low).** PadApp releases a session's `GlassSource` only after the Ring using it has unmounted: release it in a `useEffect` cleanup keyed on the session. `finish` must not call `release` directly. `draw` is never called after the phase is `closed` (A6).

**A8. Inline ring fits the window (low).** In Task 16, the `.ring-overlay` scales the ring down when the content is shorter than 460 px: `.ring-overlay .ring { transform: scale(min(1, calc((100vh - 20px) / 440px))) }`. If that `calc` isn't valid CSS for `scale`, set the scale from JS with `window.innerHeight` in `InlineRing`. Hit-testing must use unscaled coordinates: in Ring's `local()`, divide by `r.width / SIZE`.

**A9. StrictMode.** Every listener effect (PadApp `onPadOpen`, drag-drop, focus, `onJobEvent`, router subscriptions) must tolerate React StrictMode's mount → unmount → mount: unlisten in cleanup, and `padReady()` may be called twice (it's idempotent). InlineRing's `open` effect runs twice in dev, and that's harmless.

**A10. Thumbnail allowlist wording (low).** The allowlist limits `thumbnail` to files the user has dropped or picked, as defence in depth. It isn't a security boundary, because the webview can call `inspect` itself. Word the spec accordingly in Task 18.

## File structure

```
recast/
  pad.html                          # NEW second Vite entry (the pad window)
  vite.config.ts                    # MODIFY: multi-page build (index.html + pad.html)
  package.json                      # MODIFY: @dnd-kit/*, @tauri-apps/plugin-dialog
  src/
    shared/types.ts, ipc.ts         # MODIFY: new fields, commands, events
    shared/pageRanges.ts (+test)    # NEW page-range parser (mirror of Rust)
    pad/
      main.tsx                      # NEW mounts PadApp
      PadApp.tsx                    # NEW pad window: pad://open, capture, drag events
      Ring.tsx                      # NEW shared ring component (canvas + DOM overlay)
      usePad.ts                     # NEW state machine + IPC effects hook
      geometry.ts (+test)           # NEW constants, slotAngle, rsector, sector test
      hitTest.ts (+test)            # NEW point → slot / centre / none, gaps
      paging.ts (+test)             # NEW presets → pages of 6 slots
      states.ts (+test)             # NEW §4.3 reducer
      progress.ts (+test)           # NEW batch tracking from job events
      luminance.ts (+test)          # NEW per-slot luminance from captured pixels
      frame.ts (+test)              # NEW per-frame visual model (pure)
      anim.ts (+test)               # NEW easing, materialize, spring
      icons.ts                      # NEW icon name → SVG path data
      glass/backdrop.ts             # NEW capture bytes → ImageData → ImageBitmaps
      glass/renderer.ts             # NEW draws a frame model on a canvas
      pad.css                       # NEW
    companion/
      App.tsx                       # MODIFY: Tools section, drag routing
      dragRouter.ts                 # NEW one drag listener, routed to the active section
      Activity.tsx                  # MODIFY: inline ring replaces PresetPicker
      Tools.tsx                     # NEW Merge / Images → PDF / Split
      SortableList.tsx              # NEW dnd-kit pointer sorter
      companion.css                 # MODIFY
  src-tauri/
    Cargo.toml                      # MODIFY: windows features, tauri-plugin-dialog
    tauri.conf.json                 # MODIFY: pad window, CSP blob:
    capabilities/default.json       # MODIFY: pad window, dialog
    presets/presets.toml            # MODIFY: pdf-extract, GIF warning fields
    src/
      presets/mod.rs                # MODIFY: tools_only, warn_*, {range} validation
      presets/ranges.rs             # NEW page-range parser
      presets/command.rs            # MODIFY: per-range pass expansion
      presets/available.rs          # MODIFY: hide tools_only
      presets/catalog_tests.rs      # MODIFY: snapshot for pdf-extract
      jobs/mod.rs                   # MODIFY: JobOptions through enqueue/execute/retry
      history.rs                    # MODIFY: options column, schema 2
      pad_model.rs                  # MODIFY: engine_size, warning, recent paths
      thumbs.rs                     # NEW thumbnail plans + generation
      backdrop.rs                   # NEW capture → wire format
      pad.rs                        # NEW pad window controller (placement, show, poll)
      platform/win.rs               # MODIFY: capture, capture exclusion, show no-activate, transparency
      ipc.rs                        # MODIFY: new commands
      app_state.rs, lib.rs, tray.rs # MODIFY: wiring
```

**Conventions for every task**
- Rust commands run from `src-tauri/`; npm commands from the repo root. Use the Bash tool (Git Bash).
- **The machine is low on memory.** Run one cargo build at a time with `-j 2`, and never run builds in parallel agents.
- **Rust test filters go after `--`** (`cargo test -j 2 -- presets::`). A filter before `--` skips building `examples/fake_engine.rs`, and the runner and job tests then fail with "fake_engine.exe is missing".
- **Write files with the Write/Edit tools, not shell heredocs.** Heredocs have collapsed `\\` to `\` before. Re-read backslash-heavy strings (Windows paths, regexes) after writing them.
- After a preset change, update snapshots: run `INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test -j 2 -- presets::catalog_tests`, then a plain run, then read the changed `.snap` files.
- A running `recast.exe` locks `target/debug/recast.exe`. Either stop only the process you started, or build with `CARGO_TARGET_DIR="$LOCALAPPDATA/Temp/recast-p2"`.
- Commit after each task with the message given. End every commit message with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Front-end checks: `npm test` (Vitest) and `npm run build` (runs `tsc --noEmit`). Rust checks: `cargo test -j 2`, `cargo clippy -j 2 --all-targets -- -D warnings`, `cargo fmt --check`.

**Execution groups and review checkpoints**

| Group | Tasks | Then |
|---|---|---|
| A: Rust data | 1–4 | |
| B: Rust windowing | 5–7 | **Opus checkpoint review 1** |
| C: TS logic + glass | 8–13 | **Opus checkpoint review 2** |
| D: UI | 14–17 | |
| E: wrap-up | 18 | **Final Opus review** |

---

### Task 1: Dependencies, the pad window and the second Vite entry

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/lib.rs`, `package.json`, `vite.config.ts`
- Create: `pad.html`, `src/pad/main.tsx`, `src/pad/PadApp.tsx` (placeholder replaced in Task 15), `src/pad/pad.css`

- [ ] **Step 1: Rust dependencies**

In `src-tauri/Cargo.toml`, add the dialog plugin under `[dependencies]` and extend the `windows` features:

```toml
tauri-plugin-dialog = "2"
windows = { version = "0.62.2", features = ["Win32_Foundation", "Win32_Security", "Win32_System_JobObjects", "Win32_System_Threading", "Win32_System_Diagnostics_ToolHelp", "Win32_Storage_FileSystem", "Win32_Graphics_Gdi", "Win32_UI_WindowsAndMessaging", "Win32_System_Registry"] }
```

(Replace the existing `windows = …` line; keep everything else.)

- [ ] **Step 2: Front-end dependencies**

```bash
npm install @dnd-kit/core @dnd-kit/sortable @dnd-kit/utilities @tauri-apps/plugin-dialog
```

Expected: `package.json` lists all four under `dependencies`.

- [ ] **Step 3: The pad window and CSP**

In `src-tauri/tauri.conf.json`, make `app.windows` hold both windows and allow `blob:` images (thumbnails):

```json
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
      },
      {
        "label": "pad",
        "title": "Recast pad",
        "url": "pad.html",
        "width": 440,
        "height": 440,
        "resizable": false,
        "decorations": false,
        "transparent": true,
        "shadow": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": false,
        "focus": false,
        "dragDropEnabled": true
      }
    ],
    "security": { "csp": "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'" }
```

- [ ] **Step 4: Capabilities**

Replace `src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Companion and pad window permissions",
  "windows": ["companion", "pad"],
  "permissions": ["core:default", "opener:allow-open-url", "opener:allow-default-urls", "dialog:allow-open"]
}
```

- [ ] **Step 5: Register the dialog plugin**

In `src-tauri/src/lib.rs`, after `.plugin(tauri_plugin_opener::init())` add:

```rust
        .plugin(tauri_plugin_dialog::init())
```

- [ ] **Step 6: Second Vite entry**

Create `pad.html` at the repo root:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Recast pad</title>
  </head>
  <body class="pad-body">
    <div id="root"></div>
    <script type="module" src="/src/pad/main.tsx"></script>
  </body>
</html>
```

Replace `vite.config.ts`:

```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome105",
    outDir: "dist",
    rollupOptions: {
      // Relative to the project root; no node imports (there is no @types/node).
      input: { main: "index.html", pad: "pad.html" },
    },
  },
  test: { environment: "jsdom", globals: true },
});
```

- [ ] **Step 7: Placeholder pad app**

`src/pad/pad.css`:

```css
html, body.pad-body, .pad-body #root {
  margin: 0; width: 100%; height: 100%;
  background: transparent; overflow: hidden;
  user-select: none; -webkit-user-select: none;
  font: 600 11px/1.2 "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
}
```

`src/pad/PadApp.tsx`:

```tsx
export function PadApp() {
  return null;
}
```

`src/pad/main.tsx`:

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { PadApp } from "./PadApp";
import "./pad.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <PadApp />
  </StrictMode>,
);
```

- [ ] **Step 8: Verify both halves build**

```bash
npm run build
cd src-tauri && cargo build -j 2
```

Expected: `dist/index.html` and `dist/pad.html` both exist, and cargo prints `Finished`.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "chore: pad window, second Vite entry, dialog plugin, dnd-kit"
```

---

### Task 2: Page ranges, `{range}` passes and the `pdf-extract` preset

**Files:**
- Create: `src-tauri/src/presets/ranges.rs`
- Modify: `src-tauri/src/presets/mod.rs`, `src-tauri/src/presets/command.rs`, `src-tauri/src/presets/available.rs`, `src-tauri/src/presets/catalog_tests.rs`, `src-tauri/presets/presets.toml`, `src-tauri/src/jobs/mod.rs` (only the `BuildInput` literal)

- [ ] **Step 1: Write the range parser with its tests**

Create `src-tauri/src/presets/ranges.rs`:

```rust
//! Page ranges for Split PDF (spec §4.5 item 2): "1-3, 5, 8-".
//! Mirrored in src/shared/pageRanges.ts; keep both rule sets identical.

/// One range; `end: None` means "to the last page".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub start: u32,
    pub end: Option<u32>,
}

pub const MAX_RANGES: usize = 200;

impl PageRange {
    /// qpdf page-range syntax: "1-3", "5", "8-z".
    pub fn qpdf(&self) -> String {
        match self.end {
            Some(e) if e == self.start => self.start.to_string(),
            Some(e) => format!("{}-{e}", self.start),
            None => format!("{}-z", self.start),
        }
    }

    /// Output file stem: "page-5", "pages-1-3", "pages-8-end".
    pub fn file_stem(&self) -> String {
        match self.end {
            Some(e) if e == self.start => format!("page-{}", self.start),
            Some(e) => format!("pages-{}-{e}", self.start),
            None => format!("pages-{}-end", self.start),
        }
    }
}

fn num(s: &str, whole: &str) -> Result<u32, String> {
    let bad = || format!("\"{whole}\" isn't a page number or range.");
    // Digits only: u32::from_str would also accept "+5".
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let n: u32 = s.parse().map_err(|_| bad())?;
    if n == 0 {
        return Err("Pages start at 1.".into());
    }
    Ok(n)
}

/// Parse "1-3, 5, 8-". `pages`: the document's page count when known.
/// Identical ranges are kept once (they would write the same file).
pub fn parse(s: &str, pages: Option<u32>) -> Result<Vec<PageRange>, String> {
    let mut out: Vec<PageRange> = Vec::new();
    for part in s.split(',') {
        let p: String = part.chars().filter(|c| !c.is_whitespace()).collect();
        if p.is_empty() {
            continue;
        }
        let r = match p.split_once('-') {
            None => {
                let n = num(&p, &p)?;
                PageRange { start: n, end: Some(n) }
            }
            Some((a, "")) => PageRange { start: num(a, &p)?, end: None },
            Some((a, b)) => {
                let (a, b) = (num(a, &p)?, num(b, &p)?);
                if a > b {
                    return Err(format!("\"{p}\" runs backwards."));
                }
                PageRange { start: a, end: Some(b) }
            }
        };
        if let Some(total) = pages {
            let last = r.end.unwrap_or(r.start);
            if r.start > total || last > total {
                return Err(format!("This PDF has {total} pages; \"{p}\" goes past the end."));
            }
        }
        if !out.contains(&r) {
            out.push(r);
        }
    }
    if out.is_empty() {
        return Err("Enter pages, e.g. 1-3, 5, 8-".into());
    }
    if out.len() > MAX_RANGES {
        return Err(format!("At most {MAX_RANGES} ranges at a time."));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: u32, end: Option<u32>) -> PageRange {
        PageRange { start, end }
    }

    #[test]
    fn parses_singles_spans_and_open_ends() {
        assert_eq!(
            parse(" 1-3, 5 ,8- ", None).unwrap(),
            [r(1, Some(3)), r(5, Some(5)), r(8, None)]
        );
        assert_eq!(parse("2,,4", None).unwrap(), [r(2, Some(2)), r(4, Some(4))]);
        assert_eq!(parse("1-3,1-3", None).unwrap(), [r(1, Some(3))]);
    }

    #[test]
    fn rejects_bad_input_with_plain_messages() {
        assert_eq!(parse("", None).unwrap_err(), "Enter pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse(" , ", None).unwrap_err(), "Enter pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse("0", None).unwrap_err(), "Pages start at 1.");
        assert_eq!(parse("3-1", None).unwrap_err(), "\"3-1\" runs backwards.");
        assert_eq!(parse("a", None).unwrap_err(), "\"a\" isn't a page number or range.");
        assert_eq!(parse("-4", None).unwrap_err(), "\"-4\" isn't a page number or range.");
        assert_eq!(parse("1-2-3", None).unwrap_err(), "\"1-2-3\" isn't a page number or range.");
        assert_eq!(parse("+5", None).unwrap_err(), "\"+5\" isn't a page number or range.");
        assert_eq!(parse("99999999999", None).unwrap_err(), "\"99999999999\" isn't a page number or range.");
    }

    #[test]
    fn checks_against_the_page_count() {
        assert!(parse("1-10", Some(10)).is_ok());
        assert_eq!(
            parse("9-11", Some(10)).unwrap_err(),
            "This PDF has 10 pages; \"9-11\" goes past the end."
        );
        assert!(parse("11-", Some(10)).is_err());
        assert!(parse("10-", Some(10)).is_ok());
    }

    #[test]
    fn caps_the_number_of_ranges() {
        let many: Vec<String> = (1..=201).map(|n| n.to_string()).collect();
        assert_eq!(parse(&many.join(","), None).unwrap_err(), "At most 200 ranges at a time.");
    }

    #[test]
    fn qpdf_syntax_and_file_stems() {
        assert_eq!(r(1, Some(3)).qpdf(), "1-3");
        assert_eq!(r(5, Some(5)).qpdf(), "5");
        assert_eq!(r(8, None).qpdf(), "8-z");
        assert_eq!(r(1, Some(3)).file_stem(), "pages-1-3");
        assert_eq!(r(5, Some(5)).file_stem(), "page-5");
        assert_eq!(r(8, None).file_stem(), "pages-8-end");
    }
}
```

In `src-tauri/src/presets/mod.rs`, add `pub mod ranges;` next to the other `pub mod` lines.

- [ ] **Step 2: Run the parser tests**

Run: `cargo test -j 2 -- presets::ranges`
Expected: 5 tests pass.

- [ ] **Step 3: Preset fields and placeholders**

In `src-tauri/src/presets/mod.rs`:

1. Add `"range"` and `"range_name"` to `BUILTIN_PLACEHOLDERS`.
2. Add three fields to `Preset`, after `ok_exit`:

```rust
    /// Offered only from the Tools page, never in the ring (e.g. Split by page ranges).
    #[serde(default)]
    pub tools_only: bool,
    /// Warn in the ring when an input is longer than this many seconds (GIF).
    #[serde(default)]
    pub warn_over_s: Option<f64>,
    #[serde(default)]
    pub warn_text: Option<String>,
```

3. Add a helper on `impl Preset`:

```rust
    /// True if a pass repeats once per page range ({range} / {range_name}).
    pub fn per_range(&self) -> bool {
        self.passes
            .iter()
            .flatten()
            .any(|a| a.contains("{range}") || a.contains("{range_name}"))
    }
```

4. In `Catalog::validate`, inside the per-preset loop after the `out_naming` check, add:

```rust
            if p.per_range() && !(p.tools_only && p.out_naming.is_folder()) {
                errs.push(format!(
                    "{id}: {{range}} needs tools_only and a folder:<name> out_naming"
                ));
            }
            if p.warn_over_s.is_some() != p.warn_text.is_some() {
                errs.push(format!("{id}: warn_over_s and warn_text go together"));
            }
```

Add these tests to the `tests` module in `presets/mod.rs`, using the same `engines`/`keys` helpers the existing validation tests use. Read them first, and if their helper names differ, adapt the calls.

```rust
    #[test]
    fn range_placeholders_need_a_tools_only_folder_preset() {
        let base = r#"
[[preset]]
id = "x"
label = "X"
description = "d"
icon = "split"
kinds = ["pdf"]
engine = "qpdf"
bin = "qpdf"
out_ext = "pdf"
"#;
        let bins = HashMap::from([("qpdf".to_string(), vec!["qpdf".to_string()])]);
        let keys = BTreeSet::new();
        let ok = format!("{base}tools_only = true\nout_naming = \"folder:pages\"\npasses = [['--empty', '--pages', '{{in}}', '{{range}}', '--', '{{out_dir}}\\{{range_name}}.pdf']]\n");
        Catalog::parse(&ok).unwrap().validate(&bins, &keys).unwrap();
        let not_tools = format!("{base}out_naming = \"folder:pages\"\npasses = [['--pages', '{{in}}', '{{range}}', '--', '{{out_dir}}\\x.pdf']]\n");
        let errs = Catalog::parse(&not_tools).unwrap().validate(&bins, &keys).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("needs tools_only")), "{errs:?}");
    }

    #[test]
    fn warning_fields_go_together() {
        let s = r#"
[[preset]]
id = "g"
label = "GIF"
description = "d"
icon = "sparkle"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
out_ext = "gif"
out_naming = "replace_ext"
warn_over_s = 60
passes = [['-i', '{in}', '{out}']]
"#;
        let bins = HashMap::from([("ffmpeg".to_string(), vec!["ffmpeg".to_string()])]);
        let errs = Catalog::parse(s).unwrap().validate(&bins, &BTreeSet::new()).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("go together")), "{errs:?}");
    }
```

- [ ] **Step 4: Per-range pass expansion in `build_command`**

In `src-tauri/src/presets/command.rs`:

1. Add a field to `BuildInput`:

```rust
    /// Page ranges for per-range presets (empty otherwise).
    pub ranges: &'a [super::ranges::PageRange],
```

2. Replace the pass loop in `build_command` (`let mut passes = Vec::new(); for template in &preset.passes { … } Ok(passes)`) with:

```rust
    let mut passes = Vec::new();
    for template in &preset.passes {
        let repeats = template
            .iter()
            .any(|a| a.contains("{range}") || a.contains("{range_name}"));
        if repeats && b.ranges.is_empty() {
            return Err("no page ranges given".into());
        }
        let rounds: Vec<Option<&super::ranges::PageRange>> = if repeats {
            b.ranges.iter().map(Some).collect()
        } else {
            vec![None]
        };
        for range in rounds {
            let mut pass_values = values.clone();
            if let Some(r) = range {
                pass_values.insert("range".into(), r.qpdf());
                pass_values.insert("range_name".into(), r.file_stem());
            }
            let mut argv: Vec<OsString> = Vec::new();
            if preset.bin == "ffmpeg" {
                argv.extend(FFMPEG_PREFIX.iter().map(OsString::from));
            }
            for arg in template {
                if arg.contains("{inputs}") {
                    for input in &inputs {
                        let mut v = pass_values.clone();
                        v.insert("inputs".into(), input.clone());
                        argv.push(expand(arg, &v)?.into());
                    }
                } else {
                    argv.push(expand(arg, &pass_values)?.into());
                }
            }
            passes.push(argv);
        }
    }
    Ok(passes)
```

3. Add `ranges: &[],` to **every** existing `BuildInput { … }` literal: the three in `command.rs` tests, the one in `presets/catalog_tests.rs`, and the one in `jobs/mod.rs` (Task 3 replaces that last one with real ranges).

4. Add a test to `command.rs`'s `tests` module:

```rust
    #[test]
    fn a_range_pass_repeats_once_per_range() {
        use crate::presets::ranges::parse;
        let c = Catalog::parse(crate::presets::BUNDLED).unwrap();
        let p = c.get("pdf-extract").unwrap();
        let inputs = vec![PathBuf::from(r"C:\docs\report.pdf")];
        let v = vars();
        let ranges = parse("1-3, 5, 8-", None).unwrap();
        let passes = build_command(
            p,
            &BuildInput {
                inputs: &inputs,
                in_dir: Path::new(r"C:\docs"),
                out_ext: "pdf",
                vars: &v,
                ranges: &ranges,
            },
        )
        .unwrap();
        assert_eq!(passes.len(), 3);
        assert_eq!(
            argv(&passes[0]),
            ["--empty", "--pages", r"C:\docs\report.pdf", "1-3", "--", r"out\pages-1-3.pdf"]
        );
        assert_eq!(argv(&passes[2])[3], "8-z");
        assert_eq!(argv(&passes[2])[5], r"out\pages-8-end.pdf");
        let none = build_command(
            p,
            &BuildInput {
                inputs: &inputs,
                in_dir: Path::new(r"C:\docs"),
                out_ext: "pdf",
                vars: &v,
                ranges: &[],
            },
        );
        assert_eq!(none.unwrap_err(), "no page ranges given");
    }
```

- [ ] **Step 5: The catalog entries**

In `src-tauri/presets/presets.toml`, add directly after the `pdf-merge` preset:

```toml
[[preset]]
id = "pdf-extract"
label = "Split by pages"
description = "One PDF per page range"
icon = "split"
kinds = ["pdf"]
engine = "qpdf"
bin = "qpdf"
ok_exit = [0, 3]
keep_same_format = true
tools_only = true
out_ext = "pdf"
out_naming = "folder:pages"
passes = [['--empty', '--pages', '{in}', '{range}', '--', '{out_dir}\{range_name}.pdf']]
```

Then add these two lines to the `video-gif` preset, after `out_naming = "replace_ext"`:

```toml
warn_over_s = 60
warn_text = "Over a minute long: the GIF will be large and slow to make."
```

TOML literal strings (single quotes) keep `\` as-is. Re-read the `pdf-extract` line after writing it: it must contain exactly one backslash between `{out_dir}` and `{range_name}`.

- [ ] **Step 6: Hide tools-only presets from the ring**

In `src-tauri/src/presets/available.rs`, change the filter in `available` to:

```rust
        .filter(|p| !p.tools_only && applies(p, files, caps))
```

and add a test:

```rust
    #[test]
    fn tools_only_presets_never_reach_the_ring() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Pdf, "pdf", None)], &NO_CAPS));
        assert!(a.contains(&"pdf-split".to_string()));
        assert!(!a.contains(&"pdf-extract".to_string()));
    }
```

- [ ] **Step 7: Snapshot for the new preset**

`catalog_tests.rs` snapshots every preset's argv. Read how it builds `BuildInput` for each preset: `pdf-extract` needs ranges, so where the test builds its input, pass `ranges: &ranges` with `let ranges = crate::presets::ranges::parse("1-2, 4-", None).unwrap();` when `p.per_range()`, and `&[]` otherwise. Then:

```bash
INSTA_UPDATE=always INSTA_FORCE_PASS=1 cargo test -j 2 -- presets::catalog_tests
cargo test -j 2 -- presets::catalog_tests
```

Read the new `…catalog_tests__pdf-extract.snap`. It must show two passes, `1-2` → `out\pages-1-2.pdf` and `4-z` → `out\pages-4-end.pdf`. Confirm that `git status` shows no other `.snap` file changed (`video-gif`'s argv didn't change).

- [ ] **Step 8: Run all preset tests and commit**

Run: `cargo test -j 2 -- presets::`
Expected: all pass.

```bash
git add -A
git commit -m "feat(presets): page ranges, per-range passes, tools-only pdf-extract, GIF warning fields"
```

---

### Task 3: Job options through the queue, history and retry

**Files:**
- Modify: `src-tauri/src/history.rs`, `src-tauri/src/jobs/mod.rs`, `src-tauri/src/ipc.rs`

- [ ] **Step 1: History schema 2 with an `options` column (test first)**

Add this test to `history.rs`'s `tests` module:

```rust
    #[test]
    fn options_round_trip_and_old_databases_migrate() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("recast.db");
        {
            // A schema-1 database, as Plan 1 created it.
            let c = rusqlite::Connection::open(&db).unwrap();
            c.execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta VALUES ('schema_version', '1');
                 CREATE TABLE jobs (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
                   preset_id TEXT NOT NULL, inputs TEXT NOT NULL, output TEXT, status TEXT NOT NULL,
                   error_code TEXT, error_detail TEXT, stderr_tail TEXT, started_at INTEGER NOT NULL,
                   ended_at INTEGER, temp_path TEXT);
                 INSERT INTO jobs (id, preset_id, inputs, status, started_at)
                   VALUES ('old', 'pdf-split', '[\"C:\\\\a.pdf\"]', 'done', 1);",
            )
            .unwrap();
        }
        let h = History::open(&db).unwrap();
        assert_eq!(h.get("old").unwrap().unwrap().options, None);
        h.insert("new", "pdf-extract", &["C:\\b.pdf".to_string()], 2).unwrap();
        h.set_options("new", Some(r#"{"ranges":"1-3"}"#)).unwrap();
        assert_eq!(
            h.get("new").unwrap().unwrap().options.as_deref(),
            Some(r#"{"ranges":"1-3"}"#)
        );
        drop(h);
        // Opening again is a no-op migration.
        let h = History::open(&db).unwrap();
        assert!(h.get("new").unwrap().unwrap().options.is_some());
    }
```

Run: `cargo test -j 2 -- history::` and expect a compile error (`options` / `set_options` don't exist yet).

- [ ] **Step 2: Implement it**

In `history.rs`:
- `const SCHEMA_VERSION: &str = "2";`
- `COLS`: append `, options`
- `JobRecord`: add `pub options: Option<String>,` after `temp_path`, with the doc comment `/// JSON JobOptions (page ranges) or None.`
- `row()`: add `options: r.get("options")?,`
- In `migrate`, after the `CREATE TABLE` batch and before the `INSERT OR IGNORE` into `meta`:

```rust
        let has_options: bool = self
            .conn
            .prepare("SELECT 1 FROM pragma_table_info('jobs') WHERE name = 'options'")?
            .exists([])?;
        if !has_options {
            self.conn
                .execute("ALTER TABLE jobs ADD COLUMN options TEXT", [])?;
        }
```

and change the `meta` write to `INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)`.
- Add a method:

```rust
    pub fn set_options(&self, id: &str, options: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE jobs SET options = ?2 WHERE id = ?1",
            params![id, options],
        )?;
        Ok(())
    }
```

Fix any test in `history.rs` or `ipc.rs` that builds a `JobRecord` literal by adding `options: None`.

Run: `cargo test -j 2 -- history::`. Expected: all pass.

- [ ] **Step 3: `JobOptions` in the job manager (test first)**

In `jobs/mod.rs`, add near `JobEvent`:

```rust
/// Per-job choices that aren't settings (spec deviation 2): page ranges for Split by pages.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct JobOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranges: Option<String>,
}

impl JobOptions {
    fn is_empty(&self) -> bool {
        self.ranges.is_none()
    }
}
```

Add `use serde::Deserialize;` if missing. Read the existing job tests first (`CATALOG`, the fake host, the `r.mgr` harness). Then add a preset to the test `CATALOG` string that uses `{range}` the same way the other test presets use the fake engine. Copy the fields from the existing `ok` test preset and change only:

```toml
id = "ranged"
tools_only = true
out_naming = "folder:parts"
passes = [['ok', '{out_dir}\{range_name}.txt', '{range}']]
```

`fake_engine ok <path>` writes `converted` to its second argument (the trailing `{range}` argument is ignored). Then add these tests, which use the real harness (`rig()`, `r.wait_end(id)`):

```rust
    #[test]
    fn ranged_jobs_need_valid_ranges_and_keep_them_for_retry() {
        let r = rig(); // the existing jobs test harness
        let input = r.dir.path().join("doc.pdf");
        std::fs::write(&input, b"%PDF-1.7\n").unwrap();
        let err = r.mgr.start_jobs("ranged", &[input.clone()], JobOptions::default()).unwrap_err();
        assert_eq!(err, "Enter pages, e.g. 1-3, 5, 8-");
        let bad = JobOptions { ranges: Some("3-1".into()) };
        assert_eq!(r.mgr.start_jobs("ranged", &[input.clone()], bad).unwrap_err(), "\"3-1\" runs backwards.");
        let opts = JobOptions { ranges: Some("1-2, 4".into()) };
        let id = r.mgr.start_jobs("ranged", &[input.clone()], opts.clone()).unwrap().remove(0);
        let rec = lock(&r.history).get(&id).unwrap().unwrap();
        assert_eq!(rec.options.as_deref(), Some(r#"{"ranges":"1-2, 4"}"#));
        r.wait_end(&id);
        let again = r.mgr.retry(&id, false).unwrap().remove(0);
        let rec2 = lock(&r.history).get(&again).unwrap().unwrap();
        assert_eq!(rec2.options, rec.options);
    }

    #[test]
    fn options_are_refused_by_presets_that_take_none() {
        let r = rig();
        let input = r.dir.path().join("a.bin");
        std::fs::write(&input, b"x").unwrap();
        let opts = JobOptions { ranges: Some("1".into()) };
        assert_eq!(
            r.mgr.start_jobs("ok", &[input], opts).unwrap_err(),
            "This conversion doesn't take page ranges."
        );
    }
```

Run `cargo test -j 2 -- jobs::` and expect compile errors.

- [ ] **Step 4: Thread options through**

In `jobs/mod.rs`:

1. `struct Job` gains `options: JobOptions,`.
2. `pub fn start_jobs(&self, preset_id: &str, paths: &[PathBuf], options: JobOptions) -> Result<Vec<String>, String>` calls `self.enqueue(preset_id, paths, false, options)`.
3. `retry` parses the stored options and passes them on:

```rust
        let options: JobOptions = rec
            .options
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        self.enqueue(&rec.preset_id, &paths, fallback, options)
```

4. `enqueue(&self, preset_id, paths, fallback, options: JobOptions)`. After the `paths.is_empty()` check, add:

```rust
        if preset.per_range() {
            let text = options.ranges.as_deref().unwrap_or("");
            // The page count makes "past the end" an early, plain error when qpdf is installed.
            let pages = paths.first().and_then(|p| self.inner.host.pdf_pages(p));
            crate::presets::ranges::parse(text, pages)?;
        } else if !options.is_empty() {
            return Err("This conversion doesn't take page ranges.".into());
        }
```

When building each `Job`, set `options: options.clone()`. After the `history.insert(…)` call, add:

```rust
            if !job.options.is_empty() {
                let json = serde_json::to_string(&job.options).map_err(|e| e.to_string())?;
                lock(&self.inner.history)
                    .set_options(&job.id, Some(&json))
                    .map_err(|e| e.to_string())?;
            }
```

5. In `run_job` (where `build_command` is called; read it), just before `build_command`, parse the ranges again (the page count is known by now) and pass them:

```rust
        let ranges = if preset.per_range() {
            crate::presets::ranges::parse(
                job.options.ranges.as_deref().unwrap_or(""),
                self.host.pdf_pages(first),
            )
            .map_err(|e| Failure::new(SourceUnreadable, e))?
        } else {
            Vec::new()
        };
```

and set `ranges: &ranges,` in that `BuildInput` literal.

6. Update every other `start_jobs(…)` / `enqueue(…)` call in `jobs/mod.rs` tests to pass `JobOptions::default()`.

- [ ] **Step 5: IPC**

In `ipc.rs`, change `start_jobs`:

```rust
#[tauri::command]
pub async fn start_jobs(
    state: State<'_, AppState>,
    preset_id: String,
    paths: Vec<String>,
    options: Option<crate::jobs::JobOptions>,
) -> Result<Vec<String>, String> {
    let jobs = state.jobs.clone();
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        jobs.start_jobs(&preset_id, &paths, options.unwrap_or_default())
    })
    .await
}
```

Update `tests/engines_integration.rs` the same way. Every call to `start_jobs` there gets `JobOptions::default()`, and add one integration test next to the existing qpdf split test, modelled on it:

```rust
// pdf-extract: "1, 2-" on the 3-page fixture gives page-1.pdf (1 page) and pages-2-end.pdf (2 pages).
```

Assert both files exist in the `… pages` folder and use `qpdf --show-npages` (as the existing tests do) to check that the page counts are 1 and 2.

- [ ] **Step 6: Run and commit**

```bash
cargo test -j 2
cargo clippy -j 2 --all-targets -- -D warnings
```

Expected: all pass and clippy is clean. Real engines aren't required here.

```bash
git add -A
git commit -m "feat(jobs): per-job options (page ranges) stored in history schema 2 and kept on retry"
```

---

### Task 4: `PresetView` extras and the recent-paths allowlist

**Files:**
- Modify: `src-tauri/src/pad_model.rs`, `src-tauri/src/app_state.rs`, `src-tauri/src/ipc.rs`

- [ ] **Step 1: Tests first**

Add to `pad_model.rs` tests:

```rust
    #[test]
    fn gif_warns_only_for_long_videos() {
        let catalog = Catalog::parse(PRESETS).unwrap();
        let gif = catalog.get("video-gif").unwrap();
        let mut f = FileInfo::bare(std::path::Path::new(r"C:\a.mov"), Kind::Video, "mov".into(), 1);
        f.duration_s = Some(61.0);
        assert!(warning_for(gif, std::slice::from_ref(&f)).unwrap().contains("Over a minute"));
        f.duration_s = Some(60.0);
        assert_eq!(warning_for(gif, std::slice::from_ref(&f)), None);
        f.duration_s = None;
        assert_eq!(warning_for(gif, &[f]), None);
        let mp3 = catalog.get("video-mp3").unwrap();
        let mut long = FileInfo::bare(std::path::Path::new(r"C:\b.mov"), Kind::Video, "mov".into(), 1);
        long.duration_s = Some(600.0);
        assert_eq!(warning_for(mp3, &[long]), None);
    }

    #[test]
    fn recent_paths_remember_the_last_256() {
        let mut r = RecentPaths::default();
        for i in 0..300 {
            r.add(std::path::PathBuf::from(format!(r"C:\f{i}.png")));
        }
        assert!(!r.contains(std::path::Path::new(r"C:\f0.png")));
        assert!(r.contains(std::path::Path::new(r"C:\f299.png")));
        assert!(r.contains(std::path::Path::new(r"C:\f44.png")));
        r.add(std::path::PathBuf::from(r"C:\f44.png")); // re-adding refreshes, no duplicate
        assert_eq!(r.len(), 256);
    }
```

Also extend `png_gets_image_presets_with_engine_status`:

```rust
        assert!(m.presets[0].engine_size > 0);
        assert_eq!(m.presets[0].warning, None);
```

- [ ] **Step 2: Implement**

In `pad_model.rs`:

```rust
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
```

Add to `PresetView`:

```rust
    /// Download size of the engine (manifest), for "Getting FFmpeg · 84 MB".
    pub engine_size: u64,
    /// A caution shown in the ring (e.g. GIF from a long video).
    pub warning: Option<String>,
```

Add:

```rust
/// The preset's caution if any input is longer than its warn_over_s.
pub fn warning_for(preset: &crate::presets::Preset, files: &[FileInfo]) -> Option<String> {
    let limit = preset.warn_over_s?;
    files
        .iter()
        .any(|f| f.duration_s.is_some_and(|d| d > limit))
        .then(|| preset.warn_text.clone())
        .flatten()
}

/// Paths a recent `inspect` returned: the only files `thumbnail` will read (spec deviation 11).
#[derive(Default)]
pub struct RecentPaths(VecDeque<PathBuf>);

impl RecentPaths {
    const CAP: usize = 256;

    pub fn add(&mut self, p: PathBuf) {
        self.0.retain(|q| q != &p);
        self.0.push_back(p);
        while self.0.len() > Self::CAP {
            self.0.pop_front();
        }
    }

    pub fn contains(&self, p: &Path) -> bool {
        self.0.iter().any(|q| q == p)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
```

In `build`, the presets mapping needs the catalog preset for `warning_for`. `available` returns `&Preset` items, so use them directly:

```rust
            engine_size: engines
                .manifest()
                .get(&p.engine)
                .map(|e| e.size)
                .unwrap_or(0),
            warning: warning_for(p, &files),
```

Check the manifest entry's size field name in `engines/manifest.rs`; it is `size` per spec §6.3. If its type isn't `u64`, convert it with `as u64`.

- [ ] **Step 3: Wire the allowlist into `inspect`**

In `app_state.rs`, add the field `pub recent: Arc<Mutex<crate::pad_model::RecentPaths>>,` to `AppState` and initialise it with `recent: Arc::new(Mutex::new(Default::default())),`.

In `ipc.rs` `inspect`, record every file the model returns:

```rust
#[tauri::command]
pub async fn inspect(state: State<'_, AppState>, paths: Vec<String>) -> Result<PadModel, String> {
    let (catalog, engines, recent) = (
        state.catalog.clone(),
        state.engines.clone(),
        state.recent.clone(),
    );
    blocking(move || {
        let model = pad_model::build(&catalog, &engines, &paths)?;
        let mut r = lock(&recent);
        for f in &model.files {
            r.add(f.path.clone());
        }
        Ok(model)
    })
    .await
}
```

`FileInfo.path` is already absolute (`build` calls `std::path::absolute`).

- [ ] **Step 4: Run and commit**

Run: `cargo test -j 2 -- pad_model::`, then `cargo test -j 2`.
Expected: all pass.

```bash
git add -A
git commit -m "feat(pad-model): engine size, GIF length warning, recent-paths allowlist"
```

---
### Task 5: Screen capture in `platform`, and `backdrop.rs`

**Files:**
- Modify: `src-tauri/src/platform/win.rs`
- Create: `src-tauri/src/backdrop.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod backdrop;`)

`windows` 0.62 signatures (whether a parameter is `Option<…>`, `HDC` vs `Option<HDC>`, `HGDIOBJ` conversions) differ between releases. Write the calls below, then fix the types the compiler asks for, keeping the same calls and order. The docs are at `~/.cargo/registry/src/*/windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs`.

- [ ] **Step 1: The wire format (test first)**

Create `src-tauri/src/backdrop.rs`:

```rust
//! The backdrop under the pad (spec §6.8): captured in memory, never written to disk.
//! Wire format sent to the pad webview: u32 LE width, u32 LE height, then RGBA rows top-down.

pub const HEADER: usize = 8;
/// Larger captures are refused (a 440 px pad at 400% DPI is 1760 px).
pub const MAX_SIDE: u32 = 4096;

/// BGRA (GDI order, alpha undefined) → the wire format, alpha forced opaque.
pub fn encode(w: u32, h: u32, bgra: &[u8]) -> Result<Vec<u8>, String> {
    let n = (w as usize) * (h as usize) * 4;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("bad capture size {w}x{h}"));
    }
    if bgra.len() != n {
        return Err(format!("expected {n} bytes, got {}", bgra.len()));
    }
    let mut out = Vec::with_capacity(HEADER + n);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    for px in bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_to_rgba_and_forces_alpha() {
        let out = encode(2, 1, &[1, 2, 3, 0, 10, 20, 30, 7]).unwrap();
        assert_eq!(&out[..8], &[2, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&out[8..], &[3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn rejects_wrong_sizes() {
        assert!(encode(0, 1, &[]).is_err());
        assert!(encode(2, 2, &[0; 4]).is_err());
        assert!(encode(MAX_SIDE + 1, 1, &vec![0; (MAX_SIDE as usize + 1) * 4]).is_err());
    }
}
```

Add `pub mod backdrop;` to `lib.rs`, then run `cargo test -j 2 -- backdrop::` and expect 2 passes.

- [ ] **Step 2: Platform functions**

Append to `platform/win.rs`, with the imports at the top of the file:

```rust
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::WindowsAndMessaging::{
    SW_SHOWNOACTIVATE, SetWindowDisplayAffinity, ShowWindow, WDA_EXCLUDEFROMCAPTURE,
};
use windows::core::w;
```

```rust
/// A screen rectangle in physical pixels (virtual-screen coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Copy what is on screen under `r` as top-down BGRA. Windows marked
/// `exclude_from_capture` (the pad) are not in the image.
pub fn capture_region(r: ScreenRect) -> io::Result<Vec<u8>> {
    if r.w == 0 || r.h == 0 {
        return Err(io::Error::other("empty capture rect"));
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(io::Error::other("GetDC failed"));
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, r.w as i32, r.h as i32);
        let old = SelectObject(mem, bmp.into());
        let copied = BitBlt(mem, 0, 0, r.w as i32, r.h as i32, Some(screen), r.x, r.y, SRCCOPY);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: r.w as i32,
                biHeight: -(r.h as i32), // negative: top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; r.w as usize * r.h as usize * 4];
        let lines = if copied.is_ok() {
            GetDIBits(
                mem,
                bmp,
                0,
                r.h,
                Some(buf.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        copied.map_err(to_io)?;
        if lines != r.h as i32 {
            return Err(io::Error::other("GetDIBits copied too few lines"));
        }
        Ok(buf)
    }
}

/// Keep a window out of every screen capture, including Recast's own (spec §6.8).
/// `hwnd` is the raw handle value (tauri's HWND type comes from another `windows` version).
pub fn exclude_from_capture(hwnd: isize) -> io::Result<()> {
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut c_void), WDA_EXCLUDEFROMCAPTURE) }
        .map_err(to_io)
}

/// Show a window without taking focus, so an OLE drag in progress keeps going (spec §6.7).
pub fn show_no_activate(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut c_void), SW_SHOWNOACTIVATE);
    }
}

/// Windows "Transparency effects" (Settings → Personalisation → Colours). Missing value = on.
pub fn transparency_enabled() -> bool {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size as *mut u32),
        )
    };
    !r.is_ok() || value != 0
}
```

The `w!` raw-string literal must contain single backslashes. Re-read it after writing.

Add tests to `win.rs`'s `tests` module:

```rust
    #[test]
    fn captures_a_small_screen_region() {
        let px = capture_region(ScreenRect { x: 0, y: 0, w: 16, h: 8 }).unwrap();
        assert_eq!(px.len(), 16 * 8 * 4);
        assert!(capture_region(ScreenRect { x: 0, y: 0, w: 0, h: 8 }).is_err());
    }

    #[test]
    fn transparency_setting_reads_without_panicking() {
        let _ = transparency_enabled();
    }
```

(On a headless CI session `GetDC` still works on the default desktop. If this test fails only under a locked or service session, mark it `#[ignore]` with that reason in a comment and say so in the commit message.)

- [ ] **Step 3: Run and commit**

Run: `cargo test -j 2 -- platform:: backdrop::`
Expected: all pass.

```bash
git add -A
git commit -m "feat(platform): GDI region capture, capture exclusion, no-activate show, transparency setting"
```

---

### Task 6: Thumbnails

**Files:**
- Create: `src-tauri/src/thumbs.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod thumbs;`), `src-tauri/src/ipc.rs`

- [ ] **Step 1: The plan function (test first)**

Create `src-tauri/src/thumbs.rs`:

```rust
//! Thumbnails for the pad's centre disc (spec §4.2, deviation 11). Only installed
//! engines are used; nothing is ever downloaded for a thumbnail.
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::detect::{self, Kind};
use crate::engines::Engines;

/// Formats the webview decodes itself.
const NATIVE: &[&str] = &["png", "jpeg", "gif", "webp", "bmp", "avif", "ico"];
pub const PASSTHROUGH_MAX: u64 = 25 * 1024 * 1024;
const OUTPUT_MAX: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// Send the file's own bytes.
    Passthrough,
    Ffmpeg,
    Magick,
    Ghostscript,
    /// Show the type glyph.
    Glyph,
}

/// ImageMagick and Ghostscript interpret some filename characters; only plain
/// ASCII paths without %, [ or ] are handed to them.
pub fn plain_path(p: &Path) -> bool {
    p.to_str().is_some_and(|s| {
        s.is_ascii() && !s.contains(['%', '[', ']']) && !s.starts_with('-')
    })
}

/// `installed(engine_id)`: whether that engine is ready.
pub fn plan(kind: Kind, format: &str, size: u64, path: &Path, installed: &dyn Fn(&str) -> bool) -> Plan {
    match kind {
        Kind::Image if NATIVE.contains(&format) && size <= PASSTHROUGH_MAX => Plan::Passthrough,
        Kind::Image if installed("imagemagick") && plain_path(path) => Plan::Magick,
        Kind::Video if installed("ffmpeg") => Plan::Ffmpeg,
        Kind::Pdf if installed("ghostscript") && plain_path(path) => Plan::Ghostscript,
        _ => Plan::Glyph,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn plans_by_kind_format_size_and_engines() {
        let p = PathBuf::from(r"C:\x\a.png");
        let all = |_: &str| true;
        let none = |_: &str| false;
        assert_eq!(plan(Kind::Image, "png", 10, &p, &none), Plan::Passthrough);
        assert_eq!(plan(Kind::Image, "png", PASSTHROUGH_MAX + 1, &p, &all), Plan::Magick);
        assert_eq!(plan(Kind::Image, "png", PASSTHROUGH_MAX + 1, &p, &none), Plan::Glyph);
        assert_eq!(plan(Kind::Image, "heif", 10, &p, &all), Plan::Magick);
        assert_eq!(plan(Kind::Image, "heif", 10, &p, &none), Plan::Glyph);
        assert_eq!(plan(Kind::Video, "mov", 10, &p, &all), Plan::Ffmpeg);
        assert_eq!(plan(Kind::Video, "mov", 10, &p, &none), Plan::Glyph);
        assert_eq!(plan(Kind::Pdf, "pdf", 10, &p, &all), Plan::Ghostscript);
        assert_eq!(plan(Kind::Audio, "mp3", 10, &p, &all), Plan::Glyph);
        assert_eq!(plan(Kind::Text, "markdown", 10, &p, &all), Plan::Glyph);
    }

    #[test]
    fn odd_paths_skip_the_filename_interpreting_engines() {
        let all = |_: &str| true;
        assert!(plain_path(Path::new(r"C:\docs\report 1.pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\100% done.pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\[draft].pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\café.pdf")));
        assert_eq!(plan(Kind::Pdf, "pdf", 1, Path::new(r"C:\café.pdf"), &all), Plan::Glyph);
        assert_eq!(plan(Kind::Image, "heif", 1, Path::new(r"C:\[x].heic"), &all), Plan::Glyph);
    }
}
```

Add `pub mod thumbs;` to `lib.rs`. Run `cargo test -j 2 -- thumbs::` and expect 2 passes.

- [ ] **Step 2: Generation**

Append to `thumbs.rs`, above the tests:

```rust
fn is_png(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
}

fn run(program: &Path, args: &[OsString], env: &[(OsString, OsString)]) -> Option<Vec<u8>> {
    let out = crate::proc::run_full(program, args, env, TIMEOUT).ok()?;
    let ok = out.status.is_some_and(|s| s.success());
    (ok && is_png(&out.stdout) && out.stdout.len() <= OUTPUT_MAX).then_some(out.stdout)
}

fn ffmpeg_frame(ffmpeg: &Path, path: &Path, at: &str) -> Option<Vec<u8>> {
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-ss", at, "-i"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(path.as_os_str().to_owned());
    args.extend(
        ["-frames:v", "1", "-vf", "scale=176:-2", "-f", "image2pipe", "-c:v", "png", "pipe:1"]
            .iter()
            .map(OsString::from),
    );
    run(ffmpeg, &args, &[])
}

/// Thumbnail bytes for `path` (PNG, or the file itself for native formats), or None for the glyph.
pub fn make(engines: &Engines, path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    let head = detect::read_head(path).ok()?;
    let (kind, format) = detect::classify(path, &head);
    let installed = |id: &str| engines.is_ready(id);
    match plan(kind, &format, meta.len(), path, &installed) {
        Plan::Passthrough => std::fs::read(path).ok(),
        Plan::Ffmpeg => {
            let ffmpeg = engines.bin("ffmpeg", "ffmpeg")?;
            // Clips shorter than a second have no frame at 1 s.
            ffmpeg_frame(&ffmpeg, path, "1").or_else(|| ffmpeg_frame(&ffmpeg, path, "0"))
        }
        Plan::Magick => {
            let magick = engines.bin("imagemagick", "magick")?;
            let mut first = path.as_os_str().to_owned();
            first.push("[0]");
            let args: Vec<OsString> = vec![
                first,
                "-auto-orient".into(),
                "-thumbnail".into(),
                "176x176".into(),
                "png:-".into(),
            ];
            run(&magick, &args, &crate::engines::magick_env(&magick))
        }
        Plan::Ghostscript => {
            let gs = engines.bin("ghostscript", "gswin64c")?;
            let mut args: Vec<OsString> = [
                "-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m", "-r48",
                "-dFirstPage=1", "-dLastPage=1", "-dTextAlphaBits=4", "-dGraphicsAlphaBits=4",
                "-sOutputFile=-",
            ]
            .iter()
            .map(OsString::from)
            .collect();
            args.push(path.as_os_str().to_owned());
            run(&gs, &args, &[])
        }
        Plan::Glyph => None,
    }
}
```

Check `detect::classify`'s exact return type: it is `(Kind, String)`, so `&format` is a `&String` and needs `.as_str()` where `plan` expects `&str`. Check `proc::run_full`'s argument types too, and adapt the calls without changing behaviour.

- [ ] **Step 3: The command**

In `ipc.rs`:

```rust
/// Thumbnail bytes for a file a recent `inspect` returned (empty: show the glyph).
#[tauri::command]
pub async fn thumbnail(state: State<'_, AppState>, path: String) -> Result<tauri::ipc::Response, String> {
    let (engines, recent) = (state.engines.clone(), state.recent.clone());
    let bytes = blocking(move || {
        let p = std::path::absolute(&path).map_err(|e| e.to_string())?;
        if !lock(&recent).contains(&p) {
            return Err("That file wasn't dropped on Recast.".to_string());
        }
        Ok(crate::thumbs::make(&engines, &p).unwrap_or_default())
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}
```

Register `ipc::thumbnail` in `lib.rs`'s `generate_handler!`.

- [ ] **Step 4: Run and commit**

Run: `cargo test -j 2`, then `cargo clippy -j 2 --all-targets -- -D warnings`.

```bash
git add -A
git commit -m "feat: thumbnails from installed engines, limited to recently inspected files"
```

---

### Task 7: The pad window controller, its commands and the tray entry

**Files:**
- Create: `src-tauri/src/pad.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/tray.rs`

- [ ] **Step 1: Placement (test first)**

Create `src-tauri/src/pad.rs`:

```rust
//! The pad window (spec §4.4, §6.7): created hidden at startup, then positioned,
//! shown and hidden for every open. Plan 3's triggers call `open` too.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::lockutil::lock;
use crate::platform;

pub const LABEL: &str = "pad";
pub const SIZE: f64 = 440.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PadMode {
    /// Opened with files already chosen: segments are clicked; the pad takes focus.
    Click,
    /// Opened mid-drag: segments are dropped on; the pad must not take focus.
    Drag,
}

#[derive(Serialize, Clone, Debug)]
pub struct PadOpen {
    pub paths: Vec<String>,
    pub mode: PadMode,
    pub reduced_transparency: bool,
}

#[derive(Serialize, Clone, Copy)]
struct Cursor {
    x: f64,
    y: f64,
}

/// Top-left of a `size`×`size` window centred on `anchor`, kept inside `work`
/// (x, y, w, h), all in physical pixels.
pub fn place(anchor: (i32, i32), size: u32, work: (i32, i32, u32, u32)) -> (i32, i32) {
    let s = size as i32;
    let axis = |a: i32, lo: i32, len: u32| {
        let hi = lo + len as i32 - s;
        if hi < lo { lo } else { (a - s / 2).clamp(lo, hi) }
    };
    (axis(anchor.0, work.0, work.2), axis(anchor.1, work.1, work.3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_on_the_anchor_inside_the_work_area() {
        let work = (0, 0, 1920, 1040);
        assert_eq!(place((960, 520), 440, work), (740, 300));
        assert_eq!(place((10, 10), 440, work), (0, 0));
        assert_eq!(place((1915, 1035), 440, work), (1480, 600));
        // A second monitor to the left, at negative coordinates.
        assert_eq!(place((-100, 500), 880, (-2560, 0, 2560, 1400)), (-880, 60));
        // A work area smaller than the pad pins it to the top-left.
        assert_eq!(place((50, 50), 440, (0, 0, 300, 300)), (0, 0));
    }
}
```

Run `cargo test -j 2 -- pad::` (after adding `pub mod pad;` to `lib.rs`) and expect 1 pass.

- [ ] **Step 2: Open, close, readiness and the cursor poll**

Append to `pad.rs` above the tests:

```rust
/// Pad window state, managed by Tauri.
#[derive(Default)]
pub struct PadState {
    /// The pad page has registered its listeners (`pad_ready`).
    ready: Mutex<bool>,
    /// An open requested before the page was ready.
    pending: Mutex<Option<PadOpen>>,
    /// The 60 Hz cursor poll is running.
    polling: Arc<AtomicBool>,
}

fn raw_hwnd(w: &tauri::WebviewWindow) -> Option<isize> {
    w.hwnd().ok().map(|h| h.0 as isize)
}

/// Exclude the pad from capture once, at startup.
pub fn init(app: &AppHandle) {
    app.manage(PadState::default());
    if let Some(w) = app.get_webview_window(LABEL)
        && let Some(h) = raw_hwnd(&w)
        && let Err(e) = platform::exclude_from_capture(h)
    {
        log::warn!("pad: capture exclusion failed: {e}");
    }
}

/// Position the pad over the cursor and show it with these files.
pub fn open(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    let Some(w) = app.get_webview_window(LABEL) else {
        log::warn!("pad window missing");
        return;
    };
    let cursor = app.cursor_position().unwrap_or(tauri::PhysicalPosition::new(0.0, 0.0));
    let anchor = (cursor.x.round() as i32, cursor.y.round() as i32);
    if let Ok(Some(m)) = app.monitor_from_point(cursor.x, cursor.y) {
        let wa = m.work_area();
        let px = (SIZE * m.scale_factor()).round() as u32;
        let (x, y) = place(anchor, px, (wa.position.x, wa.position.y, wa.size.width, wa.size.height));
        // Position first: the size is then interpreted at the target monitor's DPI.
        let _ = w.set_position(PhysicalPosition::new(x, y));
        let _ = w.set_size(tauri::PhysicalSize::new(px, px));
    }
    let req = PadOpen {
        paths,
        mode,
        reduced_transparency: !platform::transparency_enabled(),
    };
    let st = app.state::<PadState>();
    if *lock(&st.ready) {
        let _ = app.emit_to(LABEL, "pad://open", &req);
    } else {
        *lock(&st.pending) = Some(req);
    }
    match mode {
        PadMode::Click => {
            let _ = w.show();
            let _ = w.set_focus();
        }
        PadMode::Drag => match raw_hwnd(&w) {
            Some(h) => platform::show_no_activate(h),
            None => {
                let _ = w.show();
            }
        },
    }
}

/// The pad page is listening: deliver an open that arrived earlier.
pub fn ready(app: &AppHandle) {
    let st = app.state::<PadState>();
    *lock(&st.ready) = true;
    if let Some(req) = lock(&st.pending).take() {
        let _ = app.emit_to(LABEL, "pad://open", &req);
    }
}

pub fn close(app: &AppHandle) {
    track_cursor(app, false);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// While a drag is involved, emit the cursor relative to the pad (logical px) at 60 Hz:
/// Tauri sends no positions after a drag leaves the window (spec §4.3).
pub fn track_cursor(app: &AppHandle, on: bool) {
    let st = app.state::<PadState>();
    let flag = st.polling.clone();
    if !on {
        flag.store(false, Ordering::SeqCst);
        return;
    }
    if flag.swap(true, Ordering::SeqCst) {
        return; // already polling
    }
    let app = app.clone();
    std::thread::spawn(move || {
        while flag.load(Ordering::SeqCst) {
            let Some(w) = app.get_webview_window(LABEL) else { break };
            if !w.is_visible().unwrap_or(false) {
                break;
            }
            if let (Ok(c), Ok(pos), Ok(scale)) = (app.cursor_position(), w.outer_position(), w.scale_factor()) {
                let p = Cursor {
                    x: (c.x - pos.x as f64) / scale,
                    y: (c.y - pos.y as f64) / scale,
                };
                let _ = app.emit_to(LABEL, "pad://cursor", p);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        flag.store(false, Ordering::SeqCst);
    });
}
```

If `AppHandle::cursor_position` or `monitor_from_point` have different names in the installed Tauri (`~/.cargo/registry/src/*/tauri-2.12.7/src/app.rs`), use the equivalents there. Don't call Win32 directly from `pad.rs`.

Call `pad::init(app.handle());` in `lib.rs`'s `.setup`, after `app_state::setup(app)?;`.

- [ ] **Step 3: Commands**

In `ipc.rs`:

```rust
#[tauri::command]
pub async fn pad_ready(app: AppHandle) -> Result<(), String> {
    crate::pad::ready(&app);
    Ok(())
}

#[tauri::command]
pub async fn pad_close(app: AppHandle) -> Result<(), String> {
    crate::pad::close(&app);
    Ok(())
}

#[tauri::command]
pub async fn pad_track_cursor(app: AppHandle, on: bool) -> Result<(), String> {
    crate::pad::track_cursor(&app, on);
    Ok(())
}

/// The screen under the pad window, in the backdrop wire format (spec §6.8).
/// Only the pad may ask, and only for its own rectangle.
#[tauri::command]
pub async fn capture_backdrop(window: tauri::WebviewWindow) -> Result<tauri::ipc::Response, String> {
    if window.label() != crate::pad::LABEL {
        return Err("Only the pad can capture its backdrop.".into());
    }
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let rect = crate::platform::ScreenRect { x: pos.x, y: pos.y, w: size.width, h: size.height };
    let bytes = blocking(move || {
        let bgra = crate::platform::capture_region(rect).map_err(|e| e.to_string())?;
        crate::backdrop::encode(rect.w, rect.h, &bgra)
    })
    .await
    .inspect_err(|e| log::warn!("backdrop capture failed: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}
```

Register `ipc::pad_ready`, `ipc::pad_close`, `ipc::pad_track_cursor` and `ipc::capture_backdrop` in `generate_handler!`.

- [ ] **Step 4: Tray "Convert files…"**

In `tray.rs`, add a menu item between Open and Quit and handle it:

```rust
    let convert = MenuItem::with_id(app, "convert", "Convert files…", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &convert, &quit])?;
```

```rust
            "convert" => pick_and_open_pad(app),
```

```rust
/// Plan 2's way into the pad window: choose files, then the pad opens at the cursor.
fn pick_and_open_pad(app: &AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(files) = app
            .dialog()
            .file()
            .set_title("Choose files to convert")
            .blocking_pick_files()
        else {
            return;
        };
        let paths: Vec<String> = files
            .into_iter()
            .filter_map(|f| f.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if !paths.is_empty() {
            crate::pad::open(&app, paths, crate::pad::PadMode::Click);
        }
    });
}
```

- [ ] **Step 5: Verify and commit**

```bash
cargo test -j 2
cargo clippy -j 2 --all-targets -- -D warnings
cargo fmt --check
```

Expected: all pass and both checks are clean (run `cargo fmt` first if needed).

```bash
git add -A
git commit -m "feat(pad): pad window controller, capture/thumbnail/cursor commands, tray Convert files"
```

**→ Opus checkpoint review 1 (Rust, Tasks 1–7) before Group C.**

---
### Task 8: Front-end types, IPC wrappers and the page-range parser

**Files:**
- Modify: `src/shared/types.ts`, `src/shared/ipc.ts`
- Create: `src/shared/pageRanges.ts`, `src/shared/pageRanges.test.ts`

- [ ] **Step 1: Types**

In `src/shared/types.ts`:

```ts
// inside PresetView, after min_files:
  /** Engine download size in bytes (for "Getting FFmpeg · 84 MB"). */
  engine_size: number;
  /** A caution shown in the ring, e.g. GIF from a long video. */
  warning: string | null;
```

```ts
// inside JobRecord, after temp_path:
  /** JSON JobOptions, or null. */
  options: string | null;
```

Append:

```ts
export type PadMode = "click" | "drag";

export interface PadOpen {
  paths: string[];
  mode: PadMode;
  reduced_transparency: boolean;
}

export interface JobOptions {
  /** Page ranges for "pdf-extract", e.g. "1-3, 5, 8-". */
  ranges?: string;
}
```

Fix any test fixture that builds a `JobRecord` or `PresetView` literal (for example in `src/companion/jobs.test.ts`) by adding `options: null` or `engine_size: 0, warning: null`.

- [ ] **Step 2: IPC wrappers**

In `src/shared/ipc.ts`, import `JobOptions` and `PadOpen`. Replace `startJobs` and add the new commands inside `ipc`:

```ts
  startJobs: (presetId: string, paths: string[], options?: JobOptions) =>
    invoke<string[]>("start_jobs", { presetId, paths, options: options ?? null }),
  /** Thumbnail bytes (PNG, or the image file itself); empty = show the type glyph. */
  thumbnail: (path: string) => invoke<ArrayBuffer>("thumbnail", { path }),
  /** Pad window only: the screen under the pad (u32 w, u32 h, RGBA). */
  captureBackdrop: () => invoke<ArrayBuffer>("capture_backdrop"),
  padReady: () => invoke<void>("pad_ready"),
  padClose: () => invoke<void>("pad_close"),
  padTrackCursor: (on: boolean) => invoke<void>("pad_track_cursor", { on }),
```

Append:

```ts
export const onPadOpen = (cb: (p: PadOpen) => void) => listen<PadOpen>("pad://open", (e) => cb(e.payload));
/** Cursor relative to the pad window, logical px (only while tracking). */
export const onPadCursor = (cb: (p: { x: number; y: number }) => void) =>
  listen<{ x: number; y: number }>("pad://cursor", (e) => cb(e.payload));
```

- [ ] **Step 3: Page-range parser (test first)**

Create `src/shared/pageRanges.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { parseRanges } from "./pageRanges";

const err = (s: string, pages: number | null = null) => {
  const r = parseRanges(s, pages);
  return r.ok ? null : r.error;
};

describe("parseRanges (mirror of presets/ranges.rs)", () => {
  it("parses singles, spans and open ends", () => {
    expect(parseRanges(" 1-3, 5 ,8- ", null)).toEqual({
      ok: true,
      ranges: [{ start: 1, end: 3 }, { start: 5, end: 5 }, { start: 8, end: null }],
    });
    expect(parseRanges("2,,4", null)).toEqual({ ok: true, ranges: [{ start: 2, end: 2 }, { start: 4, end: 4 }] });
    expect(parseRanges("1-3,1-3", null)).toEqual({ ok: true, ranges: [{ start: 1, end: 3 }] });
  });

  it("rejects bad input with the same messages as Rust", () => {
    expect(err("")).toBe("Enter pages, e.g. 1-3, 5, 8-");
    expect(err(" , ")).toBe("Enter pages, e.g. 1-3, 5, 8-");
    expect(err("0")).toBe("Pages start at 1.");
    expect(err("3-1")).toBe('"3-1" runs backwards.');
    expect(err("a")).toBe('"a" isn\'t a page number or range.');
    expect(err("-4")).toBe('"-4" isn\'t a page number or range.');
    expect(err("1-2-3")).toBe('"1-2-3" isn\'t a page number or range.');
    expect(err("+5")).toBe('"+5" isn\'t a page number or range.');
    expect(err("99999999999")).toBe('"99999999999" isn\'t a page number or range.');
  });

  it("checks against the page count", () => {
    expect(err("1-10", 10)).toBeNull();
    expect(err("9-11", 10)).toBe('This PDF has 10 pages; "9-11" goes past the end.');
    expect(err("11-", 10)).not.toBeNull();
    expect(err("10-", 10)).toBeNull();
  });

  it("caps the number of ranges", () => {
    const many = Array.from({ length: 201 }, (_, i) => String(i + 1)).join(",");
    expect(err(many)).toBe("At most 200 ranges at a time.");
  });
});
```

Run `npm test` and expect a FAIL (the module is missing).

- [ ] **Step 4: Implement**

Create `src/shared/pageRanges.ts`:

```ts
/** Page ranges for Split PDF. Mirrors src-tauri/src/presets/ranges.rs: keep both identical. */
export interface PageRange {
  start: number;
  /** null = to the last page. */
  end: number | null;
}

export type RangesResult = { ok: true; ranges: PageRange[] } | { ok: false; error: string };

export const MAX_RANGES = 200;
const U32_MAX = 4294967295;

function num(s: string, whole: string): number | string {
  const bad = `"${whole}" isn't a page number or range.`;
  if (!/^\d+$/.test(s)) return bad;
  const n = Number(s);
  if (n > U32_MAX) return bad;
  if (n === 0) return "Pages start at 1.";
  return n;
}

export function parseRanges(s: string, pages: number | null): RangesResult {
  const out: PageRange[] = [];
  for (const part of s.split(",")) {
    const p = part.replace(/\s+/g, "");
    if (p === "") continue;
    const dash = p.indexOf("-");
    let r: PageRange;
    if (dash < 0) {
      const n = num(p, p);
      if (typeof n === "string") return { ok: false, error: n };
      r = { start: n, end: n };
    } else {
      const a = num(p.slice(0, dash), p);
      if (typeof a === "string") return { ok: false, error: a };
      const rest = p.slice(dash + 1);
      if (rest === "") {
        r = { start: a, end: null };
      } else {
        const b = num(rest, p);
        if (typeof b === "string") return { ok: false, error: b };
        if (a > b) return { ok: false, error: `"${p}" runs backwards.` };
        r = { start: a, end: b };
      }
    }
    if (pages != null) {
      const last = r.end ?? r.start;
      if (r.start > pages || last > pages) {
        return { ok: false, error: `This PDF has ${pages} pages; "${p}" goes past the end.` };
      }
    }
    if (!out.some((x) => x.start === r.start && x.end === r.end)) out.push(r);
  }
  if (out.length === 0) return { ok: false, error: "Enter pages, e.g. 1-3, 5, 8-" };
  if (out.length > MAX_RANGES) return { ok: false, error: `At most ${MAX_RANGES} ranges at a time.` };
  return { ok: true, ranges: out };
}
```

Rust's `"1-2-3"` splits at the first `-` and then fails on `"2-3"`. The TS version does the same (`num("2-3", p)` fails the digits test).

- [ ] **Step 5: Run and commit**

Run: `npm test`, then `npm run build`.
Expected: all pass, and tsc is clean.

```bash
git add -A
git commit -m "feat(ui): pad/thumbnail/capture IPC wrappers, job options, page-range parser"
```

---

### Task 9: Ring geometry

**Files:**
- Create: `src/pad/geometry.ts`, `src/pad/geometry.test.ts`

- [ ] **Step 1: Tests**

Create `src/pad/geometry.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CX, CY, DISC_R, GAP, N, R, R0, R1, RC, STEP, TH, inSector, rsector, slotAngle } from "./geometry";

const nums = (d: string) => (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);

describe("geometry constants (spec §4.4)", () => {
  it("matches the visual spec", () => {
    expect([R, TH, GAP, RC, N]).toEqual([136, 68, 9, 17, 6]);
    expect([R0, R1, DISC_R]).toEqual([102, 170, 88]);
    expect(slotAngle(0)).toBeCloseTo(-Math.PI / 2);
    expect(slotAngle(3)).toBeCloseTo(Math.PI / 2);
    expect(STEP).toBeCloseTo(Math.PI / 3);
  });
});

describe("rsector", () => {
  const d = rsector(CX, CY, R, TH, slotAngle(0), STEP / 2, GAP, RC);

  it("is one closed path of 2 arcs, 4 rounded corners and 2 sides", () => {
    expect(d.startsWith("M")).toBe(true);
    expect(d.endsWith("Z")).toBe(true);
    expect(d.match(/A/g)?.length).toBe(2);
    expect(d.match(/Q/g)?.length).toBe(4);
    expect(d.match(/L/g)?.length).toBe(2);
  });

  it("starts on the outer radius and is symmetric about the slot's axis", () => {
    const n = nums(d);
    const [x0, y0] = n;
    expect(Math.hypot(x0 - CX, y0 - CY)).toBeCloseTo(R1, 1);
    // The outer arc's end point (after "A ro ro 0 lg 1") mirrors the start across x = CX for slot 0.
    const [x1, y1] = n.slice(2 + 5, 2 + 7);
    expect(x0 + x1).toBeCloseTo(2 * CX, 1);
    expect(y0).toBeCloseTo(y1, 1);
  });

  it("leaves the gap: the outer edge stops gap/2 short of the boundary ray", () => {
    const [x0, y0] = nums(d);
    const boundary = slotAngle(0) - STEP / 2;
    const a = Math.atan2(y0 - CY, x0 - CX);
    const perp = R1 * Math.sin(a - boundary);
    expect(perp).toBeGreaterThan(GAP / 2);
  });
});

describe("inSector", () => {
  const at = (i: number, rad: number, da = 0) => {
    const a = slotAngle(i) + da;
    return [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;
  };
  const inside = (x: number, y: number, i: number) => inSector(x, y, CX, CY, R, TH, slotAngle(i), STEP / 2, GAP);

  it("contains the slot's middle and nothing in the disc, the gap or outside", () => {
    expect(inside(...at(0, R), 0)).toBe(true);
    expect(inside(...at(2, R), 2)).toBe(true);
    expect(inside(...at(0, R), 1)).toBe(false);
    expect(inside(...at(0, 60), 0)).toBe(false);
    expect(inside(...at(0, R1 + 2), 0)).toBe(false);
    expect(inside(...at(0, R, STEP / 2), 0)).toBe(false); // on the boundary ray: the gap
  });
});
```

- [ ] **Step 2: Run, expect FAIL**

Run: `npm test -- geometry`

- [ ] **Step 3: Implement**

Create `src/pad/geometry.ts`. `rsector` is a straight port of the mockup's function (`docs/mockups/pad-liquid-glass.html`):

```ts
/** Ring geometry (spec §4.4) in logical px of the 440×440 pad. */
export const SIZE = 440;
export const CX = SIZE / 2;
export const CY = SIZE / 2;
export const R = 136;
export const TH = 68;
export const GAP = 9;
export const RC = 17;
export const N = 6;
export const R0 = R - TH / 2;
export const R1 = R + TH / 2;
export const DISC_R = R0 - 14;
export const STEP = (2 * Math.PI) / N;
/** Segment lift when targeted, px. */
export const LIFT = 9;

/** Centre angle of slot i: slot 0 at 12 o'clock, clockwise. */
export const slotAngle = (i: number) => -Math.PI / 2 + i * STEP;

/** Signed smallest difference a − b, in (−π, π]. */
export const angleDiff = (a: number, b: number) => Math.atan2(Math.sin(a - b), Math.cos(a - b));

const f = (n: number) => n.toFixed(2);

/**
 * Annular sector with parallel gaps and rounded corners (the mockup's `rsector`).
 * ca: centre angle, hs: half span, gap: total gap between neighbours, rc: corner radius.
 */
export function rsector(cx: number, cy: number, r: number, th: number, ca: number, hs: number, gap: number, rc: number): string {
  const ro = r + th / 2;
  const ri = r - th / 2;
  const ao = Math.asin(gap / 2 / ro);
  const ai = Math.asin(gap / 2 / ri);
  const a0o = ca - hs + ao, a1o = ca + hs - ao, a0i = ca - hs + ai, a1i = ca + hs - ai;
  const P = (rad: number, a: number) => `${f(cx + rad * Math.cos(a))} ${f(cy + rad * Math.sin(a))}`;
  const L = (a: number, b: number, t: number) => a + (b - a) * t;
  const lgo = a1o - a0o > Math.PI ? 1 : 0;
  const dao = rc / ro, dai = rc / ri;
  const e1 = (rad: number) => L(a1i, a1o, (rad - ri) / (ro - ri));
  const e0 = (rad: number) => L(a0i, a0o, (rad - ri) / (ro - ri));
  return (
    `M${P(ro, a0o + dao)} A${ro} ${ro} 0 ${lgo} 1 ${P(ro, a1o - dao)} Q${P(ro, a1o)} ${P(ro - rc, e1(ro - rc))} L${P(ri + rc, e1(ri + rc))} Q${P(ri, a1i)} ${P(ri, a1i - dai)}` +
    ` A${ri} ${ri} 0 ${lgo} 0 ${P(ri, a0i + dai)} Q${P(ri, a0i)} ${P(ri + rc, e0(ri + rc))} L${P(ro - rc, e0(ro - rc))} Q${P(ro, a0o)} ${P(ro, a0o + dao)} Z`
  );
}

/** A circle as path data (the disc). */
export const circlePath = (cx: number, cy: number, r: number) =>
  `M${cx - r} ${cy} a${r} ${r} 0 1 0 ${2 * r} 0 a${r} ${r} 0 1 0 ${-2 * r} 0Z`;

/**
 * Point inside the sector (ignoring the rounded corners): within the radial band,
 * on the slot's side of both boundary rays and at least gap/2 from each.
 */
export function inSector(x: number, y: number, cx: number, cy: number, r: number, th: number, ca: number, hs: number, gap: number): boolean {
  const dx = x - cx, dy = y - cy;
  const rad = Math.hypot(dx, dy);
  if (rad < r - th / 2 || rad > r + th / 2) return false;
  const rel = angleDiff(Math.atan2(dy, dx), ca);
  if (Math.abs(rel) > hs) return false;
  const toEdge = rad * Math.sin(hs - Math.abs(rel));
  return toEdge >= gap / 2;
}
```

The test's `nums(d).slice(7, 9)` reads the outer arc's end point. After `M x y` come `A ro ro 0 lg 1` (five numbers), then the end point.

- [ ] **Step 4: Run, expect PASS; commit**

Run: `npm test -- geometry`

```bash
git add -A
git commit -m "feat(pad): ring geometry (rsector port, sector test)"
```

---

### Task 10: Hit-testing and paging

**Files:**
- Create: `src/pad/hitTest.ts`, `src/pad/hitTest.test.ts`, `src/pad/paging.ts`, `src/pad/paging.test.ts`

- [ ] **Step 1: Tests**

`src/pad/hitTest.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CX, CY, R, R1, slotAngle, STEP } from "./geometry";
import { BAND_OUT, hitTest } from "./hitTest";

const polar = (rad: number, a: number) => [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;

describe("hitTest", () => {
  it("finds the centre disc, each slot, and nothing elsewhere", () => {
    expect(hitTest(CX, CY, null)).toEqual({ kind: "center" });
    for (let i = 0; i < 6; i++) expect(hitTest(...polar(R, slotAngle(i)), null)).toEqual({ kind: "slot", slot: i });
    expect(hitTest(...polar(91, slotAngle(0)), null)).toEqual({ kind: "none" }); // between disc and ring
    expect(hitTest(...polar(R1 + 20, slotAngle(1)), null)).toEqual({ kind: "slot", slot: 1 }); // generous outside
    expect(hitTest(...polar(BAND_OUT + 1, slotAngle(1)), null)).toEqual({ kind: "none" });
    expect(hitTest(2, 2, null)).toEqual({ kind: "none" });
  });

  it("treats the gaps as nothing unless the pointer is leaving one of their slots", () => {
    const boundary = slotAngle(1) + STEP / 2; // between slots 1 and 2 (3 o'clock)
    const p = polar(R, boundary);
    expect(hitTest(...p, null)).toEqual({ kind: "none" });
    expect(hitTest(...p, 1)).toEqual({ kind: "slot", slot: 1 });
    expect(hitTest(...p, 2)).toEqual({ kind: "slot", slot: 2 });
    expect(hitTest(...p, 4)).toEqual({ kind: "none" });
    // Slot 0 and slot 5 share the boundary at 11 o'clock (wrap-around).
    const wrap = polar(R, slotAngle(0) - STEP / 2);
    expect(hitTest(...wrap, 5)).toEqual({ kind: "slot", slot: 5 });
    expect(hitTest(...wrap, 0)).toEqual({ kind: "slot", slot: 0 });
  });
});
```

`src/pad/paging.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { firstActive, nextActive, paginate } from "./paging";

const p = (id: string): PresetView => ({
  id, label: id, description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
});
const kinds = (page: ReturnType<typeof paginate>[number]) =>
  page.map((s) => (s.kind === "preset" ? s.preset.id : s.kind));
const many = (n: number) => Array.from({ length: n }, (_, i) => p(`p${i + 1}`));

describe("paginate", () => {
  it("always gives pages of six slots", () => {
    expect(kinds(paginate([])[0])).toEqual(["empty", "empty", "empty", "empty", "empty", "empty"]);
    expect(kinds(paginate(many(3))[0])).toEqual(["p1", "p2", "p3", "empty", "empty", "empty"]);
    expect(paginate(many(6)).map(kinds)).toEqual([["p1", "p2", "p3", "p4", "p5", "p6"]]);
  });

  it("puts More in slot 6 and Back in slot 1 when there are more than six", () => {
    expect(paginate(many(7)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "empty", "empty", "empty"],
    ]);
    expect(paginate(many(10)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "p8", "p9", "p10"],
    ]);
    expect(paginate(many(11)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "p8", "p9", "more"],
      ["back", "p10", "p11", "empty", "empty", "empty"],
    ]);
  });
});

describe("keyboard order", () => {
  const page = paginate(many(3))[0];
  it("skips empty slots and wraps", () => {
    expect(firstActive(page)).toBe(0);
    expect(nextActive(page, 2, 1)).toBe(0);
    expect(nextActive(page, 0, -1)).toBe(2);
    expect(nextActive(page, null, 1)).toBe(0);
    expect(firstActive(paginate([])[0])).toBeNull();
  });
});
```

- [ ] **Step 2: Run, expect FAIL**

Run: `npm test -- hitTest paging`

- [ ] **Step 3: Implement**

`src/pad/hitTest.ts`:

```ts
import { angleDiff, CX, CY, DISC_R, GAP, N, R0, R1, slotAngle, STEP } from "./geometry";

export type Hit = { kind: "slot"; slot: number } | { kind: "center" } | { kind: "none" };

/** The ring answers a little inside and well outside its glass (the mockup's tolerance). */
export const BAND_IN = R0 - 8;
export const BAND_OUT = R1 + 34;

/**
 * Point (logical px in the pad) → what it is over. In a gap, the slot the pointer is
 * leaving stays targeted (no flicker); otherwise a gap is nothing.
 */
export function hitTest(x: number, y: number, current: number | null): Hit {
  const dx = x - CX, dy = y - CY;
  const rad = Math.hypot(dx, dy);
  if (rad <= DISC_R) return { kind: "center" };
  if (rad < BAND_IN || rad > BAND_OUT) return { kind: "none" };
  const a = Math.atan2(dy, dx);
  const slot = (((Math.round((a + Math.PI / 2) / STEP) % N) + N) % N);
  const rel = angleDiff(a, slotAngle(slot));
  const side = rel >= 0 ? 1 : -1;
  const boundary = slotAngle(slot) + (side * STEP) / 2;
  if (rad * Math.abs(Math.sin(a - boundary)) < GAP / 2) {
    const neighbour = (slot + side + N) % N;
    return current === slot || current === neighbour ? { kind: "slot", slot: current } : { kind: "none" };
  }
  return { kind: "slot", slot };
}
```

`src/pad/paging.ts`:

```ts
import type { PresetView } from "../shared/types";
import { N } from "./geometry";

export type Slot = { kind: "preset"; preset: PresetView } | { kind: "more" } | { kind: "back" } | { kind: "empty" };

const fill = (slots: Slot[]): Slot[] => [...slots, ...Array.from({ length: N - slots.length }, (): Slot => ({ kind: "empty" }))];
const preset = (p: PresetView): Slot => ({ kind: "preset", preset: p });

/** Presets → pages of exactly six slots (spec §4.2, deviation 7). */
export function paginate(presets: PresetView[]): Slot[][] {
  if (presets.length <= N) return [fill(presets.map(preset))];
  const pages: Slot[][] = [[...presets.slice(0, N - 1).map(preset), { kind: "more" }]];
  let rest = presets.slice(N - 1);
  while (rest.length > 0) {
    if (rest.length <= N - 1) {
      pages.push(fill([{ kind: "back" }, ...rest.map(preset)]));
      rest = [];
    } else {
      pages.push([{ kind: "back" }, ...rest.slice(0, N - 2).map(preset), { kind: "more" }]);
      rest = rest.slice(N - 2);
    }
  }
  return pages;
}

export const isActive = (s: Slot | undefined) => s != null && s.kind !== "empty";

export function firstActive(page: Slot[]): number | null {
  const i = page.findIndex(isActive);
  return i < 0 ? null : i;
}

/** Next targetable slot clockwise (dir 1) or anticlockwise (−1), wrapping. */
export function nextActive(page: Slot[], from: number | null, dir: 1 | -1): number | null {
  if (from == null) return firstActive(page);
  for (let k = 1; k <= N; k++) {
    const i = (((from + dir * k) % N) + N) % N;
    if (isActive(page[i])) return i;
  }
  return null;
}
```

- [ ] **Step 4: Run, expect PASS; commit**

```bash
npm test -- hitTest paging
git add -A
git commit -m "feat(pad): hit-testing with gap hysteresis, six-slot paging with More/Back"
```

---

### Task 11: The state machine (§4.3)

**Files:**
- Create: `src/pad/anim.ts` (timing constants and easing, used by states and frames), `src/pad/anim.test.ts`, `src/pad/states.ts`, `src/pad/states.test.ts`

- [ ] **Step 1: Timing helpers (test first)**

`src/pad/anim.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { approach, easeOutCubic, materialize, OPEN_MS } from "./anim";

describe("anim", () => {
  it("eases out and clamps", () => {
    expect(easeOutCubic(-1)).toBe(0);
    expect(easeOutCubic(1.5)).toBe(1);
    expect(easeOutCubic(0.5)).toBeCloseTo(0.875);
  });

  it("staggers the materialize by 55 ms per piece after a 120 ms delay", () => {
    expect(materialize(0, 0, false)).toBe(0);
    expect(materialize(120 + 520, 0, false)).toBe(1);
    expect(materialize(120 + 55 + 260, 1, false)).toBeCloseTo(0.875);
    expect(materialize(OPEN_MS, 6, false)).toBe(1);
    expect(materialize(0, 6, true)).toBe(1); // reduced motion: instant
  });

  it("springs at the same speed whatever the frame rate", () => {
    const at60 = approach(approach(0, 1, 16.667), 1, 16.667);
    const at30 = approach(0, 1, 33.333);
    expect(at60).toBeCloseTo(at30, 4);
    expect(approach(0, 1, 16.667)).toBeCloseTo(0.2, 3);
  });
});
```

`src/pad/anim.ts`:

```ts
/** Open animation (spec §4.4): 120 ms delay, 55 ms stagger, 520 ms ease-out-cubic. */
export const OPEN_DELAY = 120;
export const STAGGER = 55;
export const OPEN_DUR = 520;
/** Disc (index 0) plus six slots (1..6). */
export const OPEN_MS = OPEN_DELAY + STAGGER * 6 + OPEN_DUR;
/** Cross-fade when the ring pages (More/Back). */
export const PAGE_FADE_MS = 240;

export const clamp01 = (t: number) => Math.max(0, Math.min(1, t));
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const easeOutCubic = (t: number) => 1 - Math.pow(1 - clamp01(t), 3);

/** 0..1 progress of piece `index` (0 = disc, 1..6 = slots) `elapsed` ms after opening. */
export function materialize(elapsed: number, index: number, reduced: boolean): number {
  if (reduced) return 1;
  return easeOutCubic((elapsed - OPEN_DELAY - index * STAGGER) / OPEN_DUR);
}

/** The mockup's per-frame lerp(…, 0.2), made independent of the frame rate. */
export function approach(cur: number, target: number, dtMs: number, rate = 0.2): number {
  const k = 1 - Math.pow(1 - rate, dtMs / 16.667);
  return cur + (target - cur) * k;
}
```

- [ ] **Step 2: State machine tests**

`src/pad/states.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { OPEN_MS } from "./anim";
import { paginate } from "./paging";
import { closed, DONE_MS, DWELL_MS, FAILED_MS, LEAVE_MS, reduce, type PadEvent, type PadState } from "./states";

const p = (id: string): PresetView => ({
  id, label: id, description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
});
const pages = paginate(Array.from({ length: 8 }, (_, i) => p(`p${i + 1}`)));
const run = (events: PadEvent[], from: PadState = closed) => events.reduce(reduce, from);
const open = (at = 0, reducedMotion = false): PadEvent => ({ type: "open", pages, at, reducedMotion });
const slot = (s: number) => ({ kind: "slot" as const, slot: s });

describe("pad states (§4.3)", () => {
  it("opens, materializes, then idles", () => {
    const s = run([open(0)]);
    expect(s.phase).toEqual({ name: "opening", at: 0 });
    expect(s.focus).toBe(0);
    expect(run([open(0), { type: "tick", at: OPEN_MS - 1 }]).phase.name).toBe("opening");
    expect(run([open(0), { type: "tick", at: OPEN_MS }]).phase.name).toBe("idle");
    expect(run([open(0, true)]).phase.name).toBe("idle"); // reduced motion opens instantly
  });

  it("targets the slot under the pointer and lets go off the ring", () => {
    const s = run([open(), { type: "pointer", hit: slot(2), at: 5 }]);
    expect(s.phase).toEqual({ name: "targeting", slot: 2 });
    expect(s.focus).toBe(2);
    expect(run([{ type: "pointer", hit: { kind: "none" }, at: 6 }], s).phase.name).toBe("idle");
    expect(run([{ type: "pointer", hit: { kind: "center" }, at: 6 }], s).phase.name).toBe("idle");
  });

  it("runs the preset dropped on, ignores the disc, closes on a drop outside", () => {
    expect(run([open(), { type: "drop", hit: slot(1), at: 9 }]).phase).toEqual({ name: "starting", presetId: "p2" });
    expect(run([open(), { type: "drop", hit: { kind: "center" }, at: 9 }]).phase.name).toBe("opening");
    expect(run([open(), { type: "drop", hit: { kind: "none" }, at: 9 }]).phase.name).toBe("closed");
  });

  it("pages with More/Back on click or a 600 ms dwell", () => {
    const more = run([open(), { type: "drop", hit: slot(5), at: 10 }]);
    expect(more.page).toBe(1);
    expect(more.pageAt).toBe(10);
    expect(more.focus).toBe(0);
    const back = run([{ type: "drop", hit: slot(0), at: 20 }], more);
    expect(back.page).toBe(0);
    const hover = run([open(), { type: "pointer", hit: slot(5), at: 100 }, { type: "pointer", hit: slot(5), at: 400 }]);
    expect(run([{ type: "tick", at: 100 + DWELL_MS - 1 }], hover).page).toBe(0);
    expect(run([{ type: "tick", at: 100 + DWELL_MS }], hover).page).toBe(1);
    // Moving off More cancels the dwell.
    const off = run([{ type: "pointer", hit: slot(4), at: 300 }, { type: "tick", at: 2000 }], hover);
    expect(off.page).toBe(0);
  });

  it("drives the ring from the keyboard", () => {
    const s = run([open(0, true), { type: "key", key: "ArrowRight", at: 1 }, { type: "key", key: "Tab", at: 2 }]);
    expect(s.focus).toBe(2);
    expect(run([{ type: "key", key: "ShiftTab", at: 3 }], s).focus).toBe(1);
    expect(run([{ type: "key", key: "ArrowLeft", at: 3 }, { type: "key", key: "ArrowLeft", at: 4 }, { type: "key", key: "ArrowLeft", at: 5 }], s).focus).toBe(5);
    expect(run([{ type: "key", key: "Enter", at: 4 }], s).phase).toEqual({ name: "starting", presetId: "p3" });
    expect(run([{ type: "key", key: "Escape", at: 4 }], s).phase.name).toBe("closed");
  });

  it("closes when a drag stays more than 80 px away for 600 ms", () => {
    const s = run([open(0, true), { type: "outside", distance: 120, at: 1000 }]);
    expect(s.outsideSince).toBe(1000);
    expect(run([{ type: "tick", at: 1000 + LEAVE_MS - 1 }], s).phase.name).toBe("idle");
    expect(run([{ type: "tick", at: 1000 + LEAVE_MS }], s).phase.name).toBe("closed");
    const back = run([{ type: "outside", distance: 40, at: 1200 }, { type: "tick", at: 3000 }], s);
    expect(back.phase.name).toBe("idle");
    expect(run([{ type: "outside", distance: 80, at: 1000 }], run([open(0, true)])).outsideSince).toBeNull();
  });

  it("runs, then shows done for 1.2 s or failed for 2 s, then closes", () => {
    const running = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "started" }]);
    expect(running.phase.name).toBe("running");
    // While running, pointer and drops do nothing; Esc dismisses (the job keeps going).
    expect(run([{ type: "drop", hit: slot(2), at: 5 }], running).phase.name).toBe("running");
    expect(run([{ type: "key", key: "Escape", at: 5 }], running).phase.name).toBe("closed");
    const done = run([{ type: "settled", failed: null, at: 100 }], running);
    expect(done.phase).toEqual({ name: "done", at: 100 });
    expect(run([{ type: "tick", at: 100 + DONE_MS }], done).phase.name).toBe("closed");
    const failed = run([{ type: "settled", failed: "The converter reported an error", at: 100 }], running);
    expect(failed.phase).toEqual({ name: "failed", at: 100, message: "The converter reported an error" });
    expect(run([{ type: "tick", at: 100 + FAILED_MS - 1 }], failed).phase.name).toBe("failed");
    expect(run([{ type: "tick", at: 100 + FAILED_MS }], failed).phase.name).toBe("closed");
    const refused = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "startFailed", message: "No", at: 2 }]);
    expect(refused.phase).toEqual({ name: "failed", at: 2, message: "No" });
  });

  it("a new trigger while running reopens with the new files", () => {
    const running = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "started" }]);
    expect(run([open(50)], running).phase).toEqual({ name: "opening", at: 50 });
  });

  it("ignores everything but open while closed", () => {
    expect(run([{ type: "drop", hit: slot(0), at: 1 }, { type: "key", key: "Enter", at: 1 }, { type: "tick", at: 9 }])).toBe(closed);
  });
});
```

- [ ] **Step 3: Run, expect FAIL**

Run: `npm test -- states anim`

- [ ] **Step 4: Implement**

`src/pad/states.ts`:

```ts
/** The pad's state machine (spec §4.3). Pure: time comes in on events. */
import { OPEN_MS } from "./anim";
import type { Hit } from "./hitTest";
import { firstActive, isActive, nextActive, type Slot } from "./paging";

export const DWELL_MS = 600;
export const LEAVE_PX = 80;
export const LEAVE_MS = 600;
export const DONE_MS = 1200;
export const FAILED_MS = 2000;

export type Phase =
  | { name: "closed" }
  | { name: "opening"; at: number }
  | { name: "idle" }
  | { name: "targeting"; slot: number }
  | { name: "starting"; presetId: string }
  | { name: "running" }
  | { name: "done"; at: number }
  | { name: "failed"; at: number; message: string };

export interface PadState {
  phase: Phase;
  pages: Slot[][];
  page: number;
  /** When the page last changed (the symbols cross-fade). */
  pageAt: number;
  /** Keyboard focus on the current page. */
  focus: number | null;
  /** Pointer resting on More/Back since. */
  dwell: { slot: number; at: number } | null;
  /** Pointer more than LEAVE_PX outside the pad since (drag mode). */
  outsideSince: number | null;
  /** The last input was a key: draw the focus outline. */
  keyboard: boolean;
  reducedMotion: boolean;
}

export type PadKey = "ArrowLeft" | "ArrowRight" | "Tab" | "ShiftTab" | "Enter" | "Escape";

export type PadEvent =
  | { type: "open"; pages: Slot[][]; at: number; reducedMotion: boolean }
  | { type: "pointer"; hit: Hit; at: number }
  | { type: "outside"; distance: number; at: number }
  /** A drop, or a click. */
  | { type: "drop"; hit: Hit; at: number }
  | { type: "key"; key: PadKey; at: number }
  | { type: "tick"; at: number }
  | { type: "started" }
  | { type: "startFailed"; message: string; at: number }
  | { type: "settled"; failed: string | null; at: number }
  | { type: "dismiss" };

export const closed: PadState = {
  phase: { name: "closed" },
  pages: [],
  page: 0,
  pageAt: 0,
  focus: null,
  dwell: null,
  outsideSince: null,
  keyboard: false,
  reducedMotion: false,
};

const interactive = (p: Phase) => p.name === "opening" || p.name === "idle" || p.name === "targeting";
const shut = (s: PadState): PadState => ({ ...closed, reducedMotion: s.reducedMotion });

/** The slots of the page on show. */
export const currentPage = (s: PadState): Slot[] => s.pages[s.page] ?? [];

/** The targeted slot, if any. */
export const target = (s: PadState): number | null => (s.phase.name === "targeting" ? s.phase.slot : null);

function turnPage(s: PadState, page: number, at: number): PadState {
  const p = Math.max(0, Math.min(s.pages.length - 1, page));
  return { ...s, page: p, pageAt: at, focus: firstActive(s.pages[p] ?? []), dwell: null, phase: { name: "idle" } };
}

function activate(s: PadState, i: number | null, at: number): PadState {
  if (i == null) return s;
  const slot = currentPage(s)[i];
  switch (slot?.kind) {
    case "preset":
      return { ...s, dwell: null, phase: { name: "starting", presetId: slot.preset.id } };
    case "more":
      return turnPage(s, s.page + 1, at);
    case "back":
      return turnPage(s, s.page - 1, at);
    default:
      return s;
  }
}

export function reduce(s: PadState, e: PadEvent): PadState {
  if (e.type === "open") {
    return {
      ...closed,
      pages: e.pages,
      focus: firstActive(e.pages[0] ?? []),
      reducedMotion: e.reducedMotion,
      pageAt: e.at,
      phase: e.reducedMotion ? { name: "idle" } : { name: "opening", at: e.at },
    };
  }
  if (s.phase.name === "closed") return s;
  switch (e.type) {
    case "dismiss":
      return shut(s);
    case "tick": {
      const p = s.phase;
      if (p.name === "done" && e.at - p.at >= DONE_MS) return shut(s);
      if (p.name === "failed" && e.at - p.at >= FAILED_MS) return shut(s);
      if (!interactive(p)) return s;
      if (s.outsideSince != null && e.at - s.outsideSince >= LEAVE_MS) return shut(s);
      if (s.dwell && e.at - s.dwell.at >= DWELL_MS) return activate({ ...s, dwell: null }, s.dwell.slot, e.at);
      if (p.name === "opening" && e.at - p.at >= OPEN_MS) return { ...s, phase: { name: "idle" } };
      return s;
    }
    case "pointer": {
      if (!interactive(s.phase)) return s;
      const slot = e.hit.kind === "slot" && isActive(currentPage(s)[e.hit.slot]) ? e.hit.slot : null;
      if (slot == null) {
        return { ...s, dwell: null, phase: s.phase.name === "targeting" ? { name: "idle" } : s.phase };
      }
      const kind = currentPage(s)[slot].kind;
      const pages = kind === "more" || kind === "back";
      const dwell = pages ? (s.dwell?.slot === slot ? s.dwell : { slot, at: e.at }) : null;
      return { ...s, dwell, focus: slot, keyboard: false, phase: { name: "targeting", slot } };
    }
    case "outside":
      if (!interactive(s.phase)) return s;
    {
      const next = e.distance > LEAVE_PX ? (s.outsideSince ?? e.at) : null;
      return next === s.outsideSince ? s : { ...s, outsideSince: next };
    }
    case "drop":
      if (!interactive(s.phase)) return s;
      if (e.hit.kind === "center") return s;
      if (e.hit.kind === "none") return shut(s);
      return activate(s, e.hit.slot, e.at);
    case "key": {
      if (e.key === "Escape") return shut(s);
      if (!interactive(s.phase)) return s;
      const page = currentPage(s);
      if (e.key === "Enter") return activate(s, s.focus, e.at);
      const dir = e.key === "ArrowRight" || e.key === "Tab" ? 1 : -1;
      return { ...s, keyboard: true, focus: nextActive(page, s.focus, dir) };
    }
    case "started":
      return s.phase.name === "starting" ? { ...s, phase: { name: "running" } } : s;
    case "startFailed":
      return s.phase.name === "starting" ? { ...s, phase: { name: "failed", at: e.at, message: e.message } } : s;
    case "settled":
      if (s.phase.name !== "running") return s;
      return { ...s, phase: e.failed == null ? { name: "done", at: e.at } : { name: "failed", at: e.at, message: e.failed } };
  }
}
```

- [ ] **Step 5: Run, expect PASS; commit**

```bash
npm test -- states anim
git add -A
git commit -m "feat(pad): §4.3 state machine, open-animation timing"
```

---

### Task 12: Batch progress and backdrop luminance

**Files:**
- Create: `src/pad/progress.ts`, `src/pad/progress.test.ts`, `src/pad/luminance.ts`, `src/pad/luminance.test.ts`

- [ ] **Step 1: Tests**

`src/pad/progress.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { JobEvent, JobStatus } from "../shared/types";
import { emptyBatch, label, view, withEvent, withIds } from "./progress";

const ev = (job_id: string, status: JobStatus, pct: number | null = null, message?: string): JobEvent => ({
  job_id, batch_id: "b", batch_size: 2, preset_id: "video-mp3", preset_label: "MP3", file: `${job_id}.mov`,
  status, pct, eta_s: null, out_path: null, error: message ? { code: "ENGINE_FAILED", message } : null,
});

describe("batch progress", () => {
  it("keeps events that arrive before start_jobs returns the ids", () => {
    let b = withEvent(emptyBatch(), ev("a", "queued"));
    b = withEvent(b, ev("other", "running", 0.5)); // a job from Activity: ignored once ids are known
    b = withIds(b, ["a"]);
    expect(view(b)).toMatchObject({ total: 1, finished: 0, settled: false, pct: null });
    b = withEvent(b, ev("a", "running", 0.42));
    expect(view(b).pct).toBeCloseTo(0.42);
    b = withEvent(b, ev("a", "done", null));
    expect(view(b)).toMatchObject({ settled: true, failed: null, pct: 1 });
  });

  it("aggregates a batch and reports the first failure", () => {
    let b = withIds(emptyBatch(), ["a", "b"]);
    b = withEvent(b, ev("a", "done"));
    b = withEvent(b, ev("b", "running", 0.5));
    expect(view(b)).toMatchObject({ total: 2, finished: 1, settled: false });
    expect(view(b).pct).toBeCloseTo(0.75);
    b = withEvent(b, ev("b", "failed", null, "Couldn't read the video."));
    expect(view(b)).toMatchObject({ settled: true, failed: "Couldn't read the video." });
  });

  it("counts skipped as success and cancelled as a failure", () => {
    const b = withEvent(withEvent(withIds(emptyBatch(), ["a", "b"]), ev("a", "skipped")), ev("b", "cancelled"));
    expect(view(b)).toMatchObject({ settled: true, failed: "Cancelled." });
  });

  it("labels the disc", () => {
    const one = withIds(emptyBatch(), ["a"]);
    expect(label(view(withEvent(one, ev("a", "running", 0.42))), null)).toBe("42%");
    expect(label(view(withEvent(one, ev("a", "running", null))), null)).toBe("Working…");
    expect(label(view(withEvent(one, ev("a", "waiting_engine"))), { engine_name: "FFmpeg", engine_size: 88_080_384 })).toBe("Getting FFmpeg · 84.0 MB");
    const two = withEvent(withEvent(withIds(emptyBatch(), ["a", "b"]), ev("a", "done")), ev("b", "running", 0.5));
    expect(label(view(two), null)).toBe("1 of 2 · 75%");
  });
});
```

`src/pad/luminance.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { SIZE } from "./geometry";
import { discLuminance, isLight, luma, slotLuminance, type Pixels } from "./luminance";

function image(scale: number, color: (x: number, y: number) => number): Pixels {
  const w = SIZE * scale, h = SIZE * scale;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const v = color(x / scale, y / scale);
      data.set([v, v, v, 255], (y * w + x) * 4);
    }
  return { data, width: w, height: h };
}

describe("luminance", () => {
  it("uses Rec. 709 luma on 0..1", () => {
    expect(luma(255, 255, 255)).toBeCloseTo(1);
    expect(luma(0, 0, 0)).toBe(0);
    expect(luma(0, 255, 0)).toBeCloseTo(0.7152);
  });

  it("averages what is under each slot, at any DPI", () => {
    for (const scale of [1, 1.5, 2]) {
      const leftWhite = image(scale, (x) => (x < SIZE / 2 ? 255 : 0));
      // Slots 4 and 5 are on the left (8 and 10 o'clock); 1 and 2 on the right.
      expect(isLight(slotLuminance(leftWhite, scale, 4))).toBe(true);
      expect(isLight(slotLuminance(leftWhite, scale, 5))).toBe(true);
      expect(isLight(slotLuminance(leftWhite, scale, 1))).toBe(false);
      expect(isLight(slotLuminance(leftWhite, scale, 2))).toBe(false);
    }
    expect(slotLuminance(image(1, () => 255), 1, 0)).toBeCloseTo(1);
    expect(discLuminance(image(1, () => 0), 1)).toBe(0);
  });

  it("switches at 0.6 exactly as the spec says (> 0.6 is light)", () => {
    expect(isLight(0.6)).toBe(false);
    expect(isLight(0.61)).toBe(true);
  });
});
```

- [ ] **Step 2: Run, expect FAIL**

Run: `npm test -- progress luminance`

- [ ] **Step 3: Implement**

`src/pad/progress.ts`:

```ts
/** What the pad shows while its jobs run (spec §4.3 running state, batch indicator). */
import { formatBytes } from "../shared/format";
import type { JobEvent, JobStatus } from "../shared/types";

export interface BatchJob {
  status: JobStatus;
  pct: number | null;
  message: string | null;
}

export interface Batch {
  /** null until start_jobs returns. */
  ids: string[] | null;
  /** Latest event per job, including ones that came before the ids were known. */
  jobs: Record<string, BatchJob>;
}

export interface BatchView {
  total: number;
  finished: number;
  pct: number | null;
  waitingEngine: boolean;
  settled: boolean;
  /** The first failure's message, once settled with a failure. */
  failed: string | null;
}

const ENDED: JobStatus[] = ["done", "skipped", "failed", "cancelled", "interrupted"];
const BAD: JobStatus[] = ["failed", "cancelled", "interrupted"];

export const emptyBatch = (): Batch => ({ ids: null, jobs: {} });

export function withEvent(b: Batch, e: JobEvent): Batch {
  if (b.ids && !b.ids.includes(e.job_id)) return b;
  return { ...b, jobs: { ...b.jobs, [e.job_id]: { status: e.status, pct: e.pct, message: e.error?.message ?? null } } };
}

export function withIds(b: Batch, ids: string[]): Batch {
  const jobs: Record<string, BatchJob> = {};
  for (const id of ids) if (b.jobs[id]) jobs[id] = b.jobs[id];
  return { ids, jobs };
}

export function view(b: Batch): BatchView {
  const ids = b.ids ?? [];
  const list = ids.map((id) => b.jobs[id]);
  const finished = list.filter((j) => j && ENDED.includes(j.status)).length;
  const settled = ids.length > 0 && finished === ids.length;
  const bad = list.find((j) => j && BAD.includes(j.status));
  let pct: number | null;
  if (ids.length === 0) pct = null;
  else if (ids.length === 1) {
    const j = list[0];
    pct = j && ENDED.includes(j.status) ? 1 : j?.status === "running" ? j.pct : null;
  } else {
    const running = list.reduce((a, j) => a + (j && j.status === "running" ? (j.pct ?? 0) : 0), 0);
    pct = (finished + running) / ids.length;
  }
  return {
    total: ids.length,
    finished,
    pct,
    waitingEngine: list.some((j) => j?.status === "waiting_engine"),
    settled,
    failed: settled && bad ? (bad.message ?? (bad.status === "cancelled" ? "Cancelled." : "Couldn't convert.")) : null,
  };
}

/** The disc's status line. `engine` describes the running preset's engine. */
export function label(v: BatchView, engine: { engine_name: string; engine_size: number } | null): string {
  if (v.waitingEngine && engine) return `Getting ${engine.engine_name} · ${formatBytes(engine.engine_size)}`;
  const pct = v.pct != null ? `${Math.round(v.pct * 100)}%` : null;
  if (v.total > 1) return [`${v.finished} of ${v.total}`, pct].filter(Boolean).join(" · ");
  return pct ?? "Working…";
}
```

`src/pad/luminance.ts`:

```ts
/** Per-slot backdrop luminance for light/dark adaptation (spec §4.4, deviation 4). */
import { CX, CY, DISC_R, GAP, N, R, R1, STEP, TH, inSector, slotAngle } from "./geometry";

export interface Pixels {
  data: Uint8ClampedArray<ArrayBuffer>;
  width: number;
  height: number;
}

export const LIGHT_THRESHOLD = 0.6;
/** Sample every 4 logical px. */
const STEP_PX = 4;

export const luma = (r: number, g: number, b: number) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
export const isLight = (l: number) => l > LIGHT_THRESHOLD;

function mean(px: Pixels, scale: number, inside: (x: number, y: number) => boolean, half: number): number {
  let sum = 0, n = 0;
  for (let y = CY - half; y <= CY + half; y += STEP_PX)
    for (let x = CX - half; x <= CX + half; x += STEP_PX) {
      if (!inside(x, y)) continue;
      const px_ = Math.min(px.width - 1, Math.max(0, Math.round(x * scale)));
      const py = Math.min(px.height - 1, Math.max(0, Math.round(y * scale)));
      const o = (py * px.width + px_) * 4;
      sum += luma(px.data[o], px.data[o + 1], px.data[o + 2]);
      n++;
    }
  return n ? sum / n : 0;
}

/** Mean luma (0..1) of the backdrop under slot i at rest. `scale`: physical px per logical px. */
export const slotLuminance = (px: Pixels, scale: number, i: number) =>
  mean(px, scale, (x, y) => inSector(x, y, CX, CY, R, TH, slotAngle(i), STEP / 2, GAP), R1);

export const discLuminance = (px: Pixels, scale: number) =>
  mean(px, scale, (x, y) => Math.hypot(x - CX, y - CY) <= DISC_R, DISC_R);

export const allSlotLuminance = (px: Pixels, scale: number) =>
  Array.from({ length: N }, (_, i) => slotLuminance(px, scale, i));
```

- [ ] **Step 4: Run, expect PASS; commit**

```bash
npm test -- progress luminance
git add -A
git commit -m "feat(pad): batch progress tracking and per-slot backdrop luminance"
```

---

### Task 13: Frame model, icons, backdrop preparation and the canvas renderer

**Files:**
- Create: `src/pad/frame.ts`, `src/pad/frame.test.ts`, `src/pad/icons.ts`, `src/pad/glass/backdrop.ts`, `src/pad/glass/backdrop.test.ts`, `src/pad/glass/renderer.ts`

- [ ] **Step 1: Frame model tests**

`src/pad/frame.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { OPEN_MS } from "./anim";
import { buildFrame, restSprings, stepSprings, type FrameInput } from "./frame";
import { paginate } from "./paging";

const p = (id: string, installed = true, warning: string | null = null): PresetView => ({
  id, label: id.toUpperCase(), description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: installed ? { state: "installed", version: "1", update_available: false } : { state: "not_installed" },
  min_files: 1, engine_size: 1, warning,
});
const page = paginate([p("a"), p("b", false), p("c", true, "Big")])[0];
const base = (over: Partial<FrameInput> = {}): FrameInput => ({
  now: OPEN_MS, openedAt: 0, page, pageAt: -1000, target: null, focus: null, showFocus: false,
  springs: restSprings(), lum: [0.1, 0.1, 0.9, 0.1, 0.1, 0.1], discLum: 0.1, solid: null,
  reducedMotion: false, pointer: null, retract: 0, progress: null, ...over,
});

describe("buildFrame", () => {
  it("shows a fully materialized ring at rest", () => {
    const f = buildFrame(base());
    expect(f.slots).toHaveLength(6);
    expect(f.slots[0].mag).toBeCloseTo(1.1);
    expect(f.slots[0].glassAlpha).toBeCloseTo(1);
    expect(f.slots[0].symbolColor).toBe("#fff");
    expect(f.slots[2].light).toBe(true);
    expect(f.slots[2].symbolColor).toBe("#1d1d1f");
    expect(f.slots[3].symbolAlpha).toBe(0); // empty slot
    expect(f.slots[1].badge).toBe("download");
    expect(f.slots[2].badge).toBe("warning");
  });

  it("starts the open animation small and unmagnified", () => {
    const f = buildFrame(base({ now: 0 }));
    expect(f.slots[0].glassAlpha).toBe(0);
    expect(f.slots[0].mag).toBe(1);
  });

  it("lifts and tints the targeted slot, white symbols even over light", () => {
    const springs = restSprings();
    springs[2] = { h: 1, off: 9 };
    const f = buildFrame(base({ target: 2, springs }));
    const s = f.slots[2];
    expect(s.tint).toBe("rgba(10,132,255,0.800)");
    expect(s.symbolColor).toBe("#fff");
    expect(s.mag).toBeCloseTo(1.15);
    expect(Math.hypot(s.sx - 220, s.sy - 220)).toBeCloseTo(136 + 9, 0);
  });

  it("reduced motion: tint only, no lift or scale", () => {
    const springs = restSprings();
    springs[0] = { h: 1, off: 9 };
    const s = buildFrame(base({ reducedMotion: true, target: 0, springs })).slots[0];
    expect(s.tint).toBe("rgba(10,132,255,0.450)");
    expect(s.symbolScale).toBe(1);
    expect(Math.hypot(s.sx - 220, s.sy - 220)).toBeCloseTo(136, 0);
  });

  it("uses the solid surface when there is no backdrop", () => {
    const f = buildFrame(base({ lum: null, discLum: null, solid: "light" }));
    expect(f.slots[0].light).toBe(true);
    expect(f.solid).toBe("light");
  });

  it("retracts the segments while running and draws a progress rim", () => {
    const f = buildFrame(base({ retract: 1, progress: { pct: 0.5, spin: 0 } }));
    expect(f.slots[0].glassAlpha).toBe(0);
    expect(f.ring).toEqual({ pct: 0.5, spin: 0 });
  });
});

describe("springs", () => {
  it("approach the target slot and settle back", () => {
    let s = restSprings();
    for (let k = 0; k < 60; k++) s = stepSprings(s, 1, 16.667, false);
    expect(s[1].h).toBeCloseTo(1, 2);
    expect(s[1].off).toBeCloseTo(9, 1);
    expect(s[0].h).toBe(0);
    const reduced = stepSprings(restSprings(), 1, 16.667, true);
    expect(reduced[1].off).toBe(0);
  });
});
```

- [ ] **Step 2: Run, expect FAIL**

Run: `npm test -- frame`

- [ ] **Step 3: Implement `frame.ts`**

```ts
/** One frame of the ring as numbers (spec §4.4, the mockup's regular()). Pure. */
import { approach, clamp01, easeOutCubic, lerp, materialize, PAGE_FADE_MS } from "./anim";
import { CX, CY, DISC_R, GAP, LIFT, N, R, RC, STEP, TH, rsector, slotAngle } from "./geometry";
import { isLight } from "./luminance";
import type { Slot } from "./paging";

export const BLUE = "10,132,255";
export interface Spring { h: number; off: number }
export const restSprings = (): Spring[] => Array.from({ length: N }, () => ({ h: 0, off: 0 }));

export function stepSprings(s: Spring[], target: number | null, dt: number, reduced: boolean): Spring[] {
  return s.map((sp, i) => {
    const on = target === i;
    return {
      h: on || sp.h > 0.001 ? approach(sp.h, on ? 1 : 0, dt) : 0,
      off: reduced ? 0 : on || sp.off > 0.01 ? approach(sp.off, on ? LIFT : 0, dt) : 0,
    };
  });
}

export const springsSettled = (s: Spring[], target: number | null) =>
  s.every((sp, i) => Math.abs(sp.h - (target === i ? 1 : 0)) < 0.002 && Math.abs(sp.off - (target === i ? LIFT : 0)) < 0.02);

export interface FrameInput {
  now: number;
  openedAt: number;
  page: Slot[];
  pageAt: number;
  target: number | null;
  focus: number | null;
  showFocus: boolean;
  springs: Spring[];
  /** Per-slot backdrop luminance; null when there's no backdrop. */
  lum: number[] | null;
  discLum: number | null;
  /** The reduced-transparency surface when there is no backdrop. */
  solid: "dark" | "light" | null;
  reducedMotion: boolean;
  pointer: { x: number; y: number } | null;
  /** 0..1: segments retract into the disc while running. */
  retract: number;
  progress: { pct: number | null; spin: number } | null;
}

export interface SlotFrame {
  index: number;
  slot: Slot;
  path: string;
  /** Glass magnification origin and symbol centre (lifted). */
  sx: number;
  sy: number;
  mag: number;
  glassAlpha: number;
  light: boolean;
  tint: string;
  glowAlpha: number;
  innerAlpha: number;
  rimAlpha: number;
  symbolColor: string;
  symbolScale: number;
  symbolAlpha: number;
  focused: boolean;
  badge: "download" | "warning" | null;
}

export interface Frame {
  slots: SlotFrame[];
  disc: { r: number; m: number; mag: number; light: boolean };
  glow: { x: number; y: number } | null;
  solid: "dark" | "light" | null;
  ring: { pct: number | null; spin: number } | null;
}

export function buildFrame(f: FrameInput): Frame {
  const elapsed = f.now - f.openedAt;
  const fadeIn = f.reducedMotion ? 1 : easeOutCubic((f.now - f.pageAt) / PAGE_FADE_MS);
  const keep = 1 - f.retract;
  const slots = f.page.map((slot, i): SlotFrame => {
    const m = materialize(elapsed, i + 1, f.reducedMotion) * keep;
    const sp = f.springs[i] ?? { h: 0, off: 0 };
    const h = sp.h;
    const hg = f.reducedMotion ? 0 : h; // geometry follows the spring only with motion
    const ca = slotAngle(i);
    const hs = (STEP / 2) * (1 + 0.04 * hg) * lerp(0.55, 1, m);
    const th = TH * (1 + 0.08 * hg) * lerp(0.8, 1, m);
    const off = f.reducedMotion ? 0 : sp.off;
    const r = lerp(DISC_R, R, keep);
    const cx = CX + off * Math.cos(ca), cy = CY + off * Math.sin(ca);
    const light = f.lum ? isLight(f.lum[i]) : f.solid === "light";
    const empty = slot.kind === "empty";
    const tint = h > 0.02
      ? `rgba(${BLUE},${((light ? 0.8 : 0.45) * h).toFixed(3)})`
      : `rgba(255,255,255,${((light ? 0.12 : 0.06) * m).toFixed(3)})`;
    const badge = slot.kind === "preset"
      ? slot.preset.engine_status.state !== "installed" ? "download" : slot.preset.warning ? "warning" : null
      : null;
    return {
      index: i,
      slot,
      path: rsector(cx, cy, r, Math.max(0.01, th), ca, hs, GAP, RC),
      sx: cx + r * Math.cos(ca),
      sy: cy + r * Math.sin(ca),
      mag: lerp(1, 1.1 + 0.05 * hg, m),
      glassAlpha: empty ? 0.5 * m : m,
      light,
      tint: empty ? `rgba(255,255,255,${(0.03 * m).toFixed(3)})` : tint,
      glowAlpha: empty ? 0 : h * (light ? 0.3 : 0.9),
      innerAlpha: (light ? 0.6 : 0.32 + 0.2 * h) * m,
      rimAlpha: m,
      symbolColor: light && h < 0.5 ? "#1d1d1f" : "#fff",
      symbolScale: 1 + 0.1 * hg,
      symbolAlpha: empty ? 0 : m * fadeIn,
      focused: f.showFocus && f.focus === i,
      badge,
    };
  });
  const m0 = materialize(elapsed, 0, f.reducedMotion);
  return {
    slots,
    disc: {
      r: DISC_R,
      m: m0,
      mag: lerp(1, 1.06, m0),
      light: f.discLum != null ? isLight(f.discLum) : f.solid === "light",
    },
    glow: f.pointer,
    solid: f.solid,
    ring: f.progress,
  };
}

/** Running: segments slide into the disc over ~300 ms (instant with reduced motion). */
export const stepRetract = (cur: number, running: boolean, dt: number, reduced: boolean) =>
  reduced ? (running ? 1 : 0) : clamp01(approach(cur, running ? 1 : 0, dt, 0.25));
```

- [ ] **Step 4: Run, expect PASS**

Run: `npm test -- frame`. If `materialize(OPEN_MS, …)` rounds `glassAlpha` to 0.9999…, `toBeCloseTo` already allows that.

- [ ] **Step 5: Icons**

`src/pad/icons.ts`. The paths use a 24 × 24 viewBox and are drawn stroked (1.75):

```ts
/** Line icons (24×24, stroked 1.75) for preset `icon` names and pad glyphs. */
const circle = (cx: number, cy: number, r: number) => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
const rect = (x: number, y: number, w: number, h: number, r: number) =>
  `M${x + r} ${y}h${w - 2 * r}a${r} ${r} 0 0 1 ${r} ${r}v${h - 2 * r}a${r} ${r} 0 0 1 ${-r} ${r}h${-(w - 2 * r)}a${r} ${r} 0 0 1 ${-r} ${-r}v${-(h - 2 * r)}a${r} ${r} 0 0 1 ${r} ${-r}z`;

const ICONS: Record<string, string> = {
  music: "M9 18V5l12-2v13" + circle(6, 18, 3) + circle(18, 16, 3),
  film: rect(3, 3, 18, 18, 3) + "M7.5 3v18M16.5 3v18M3 12h18M3 7.5h4.5M3 16.5h4.5M16.5 7.5H21M16.5 16.5H21",
  sparkle: "M12 3.5l1.7 5.1 5.1 1.7-5.1 1.7L12 17.1l-1.7-5.1-5.1-1.7 5.1-1.7zM19 15.5l.6 1.9 1.9.6-1.9.6-.6 1.9-.6-1.9-1.9-.6 1.9-.6z",
  globe: circle(12, 12, 9) + "M3 12h18M12 3c2.5 2.6 3.7 5.6 3.7 9s-1.2 6.4-3.7 9c-2.5-2.6-3.7-5.6-3.7-9S9.5 5.6 12 3z",
  shrink: "M4 14h6v6M20 10h-6V4M14 10l6.5-6.5M3.5 20.5L10 14",
  box: "M21 8l-9-5-9 5v8l9 5 9-5V8zM3 8l9 5 9-5M12 13v8",
  wave: "M3 12h1M7 8v8M11 5v14M15 8v8M19 10v4",
  image: rect(3, 5, 18, 14, 2) + circle(8.5, 10, 1.5) + "M21 16l-5-5-8 8",
  grid: rect(3, 3, 7, 7, 1.5) + rect(14, 3, 7, 7, 1.5) + rect(3, 14, 7, 7, 1.5) + rect(14, 14, 7, 7, 1.5),
  doc: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h6",
  stack: "M12 3l9 5-9 5-9-5 9-5zM3 12l9 5 9-5M3 16l9 5 9-5",
  split: "M8 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3M16 3h3a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-3M12 2v20",
  book: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20V3H6.5A2.5 2.5 0 0 0 4 5.5v14zM6.5 17A2.5 2.5 0 0 0 4 19.5 2.5 2.5 0 0 0 6.5 22H20v-5",
  text: "M4 6h16M4 10h16M4 14h10M4 18h12",
  more: circle(5, 12, 1.2) + circle(12, 12, 1.2) + circle(19, 12, 1.2),
  back: "M15 5l-7 7 7 7",
  download: "M12 4v11M7 10l5 5 5-5M5 20h14",
  warning: "M12 3l10 18H2L12 3zM12 10v5M12 18h.01",
  check: "M5 12l5 5 9-10",
};

export const iconPath = (name: string): string => ICONS[name] ?? ICONS.doc;

/** The disc's type glyph when there is no thumbnail. */
export const kindIcon = (kind: string): string =>
  ({ video: "film", audio: "music", image: "image", pdf: "doc", text: "text" } as Record<string, string>)[kind] ?? "doc";
```

- [ ] **Step 6: Backdrop decoding (test first) and preparation**

`src/pad/glass/backdrop.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { decode } from "./backdrop";

describe("decode", () => {
  it("reads width, height and RGBA rows", () => {
    const buf = new Uint8Array([2, 0, 0, 0, 1, 0, 0, 0, 3, 2, 1, 255, 30, 20, 10, 255]).buffer;
    const px = decode(buf);
    expect([px.width, px.height]).toEqual([2, 1]);
    expect(Array.from(px.data)).toEqual([3, 2, 1, 255, 30, 20, 10, 255]);
  });

  it("refuses truncated data", () => {
    expect(() => decode(new Uint8Array([2, 0, 0, 0, 1, 0, 0, 0, 1]).buffer)).toThrow();
    expect(() => decode(new Uint8Array([1, 0]).buffer)).toThrow();
  });
});
```

`src/pad/glass/backdrop.ts`:

```ts
/** Captured backdrop → pixels for luminance + pre-processed glass bitmaps (spec §4.4, §6.8). */
import { SIZE } from "../geometry";
import type { Pixels } from "../luminance";

export interface GlassSource {
  /** Unblurred (the start of the open animation). */
  raw: ImageBitmap;
  /** Over dark backdrops: blur 5, saturate 1.6, brightness 1.08. */
  dark: ImageBitmap;
  /** Over light backdrops: blur 6, saturate 1.2, brightness 1.0. */
  light: ImageBitmap;
  pixels: Pixels;
  /** Physical px per logical px. */
  scale: number;
}

export function decode(buf: ArrayBuffer): Pixels {
  if (buf.byteLength < 8) throw new Error("backdrop: no header");
  const v = new DataView(buf);
  const width = v.getUint32(0, true), height = v.getUint32(4, true);
  if (buf.byteLength !== 8 + width * height * 4) throw new Error("backdrop: wrong length");
  return { data: new Uint8ClampedArray(buf, 8), width, height };
}

async function filtered(src: ImageBitmap, filter: string): Promise<ImageBitmap> {
  const c = new OffscreenCanvas(src.width, src.height);
  const x = c.getContext("2d")!;
  x.filter = filter;
  x.drawImage(src, 0, 0);
  return c.transferToImageBitmap();
}

/** Done once per open; the per-frame work only clips and transforms these. */
export async function prepare(buf: ArrayBuffer): Promise<GlassSource> {
  const pixels = decode(buf);
  const scale = pixels.width / SIZE;
  const raw = await createImageBitmap(new ImageData(pixels.data, pixels.width, pixels.height));
  const [dark, light] = await Promise.all([
    filtered(raw, `blur(${5 * scale}px) saturate(1.6) brightness(1.08)`),
    filtered(raw, `blur(${6 * scale}px) saturate(1.2) brightness(1)`),
  ]);
  return { raw, dark, light, pixels, scale };
}

/** Drop the snapshot when the pad closes (§8: it never outlives the pad). */
export function release(g: GlassSource | null) {
  g?.raw.close();
  g?.dark.close();
  g?.light.close();
}
```

- [ ] **Step 7: The renderer**

`src/pad/glass/renderer.ts` has no unit tests (jsdom has no canvas). The pure parts are already tested in `frame.ts`, and Task 18 checks it by eye and by frame time.

```ts
/** Draws one Frame onto the pad canvas with Canvas 2D (spec §4.4 rendering strategy). */
import type { Frame, SlotFrame } from "../frame";
import { BLUE } from "../frame";
import { CX, CY, SIZE, TH, circlePath, slotAngle } from "../geometry";
import { iconPath, kindIcon } from "../icons";
import type { GlassSource } from "./backdrop";

const SOLID = { dark: "rgba(43,43,48,0.92)", light: "rgba(242,242,245,0.92)" } as const;
const pathCache = new Map<string, Path2D>();
const P = (d: string) => {
  let p = pathCache.get(d);
  if (!p) {
    if (pathCache.size > 512) pathCache.clear();
    p = new Path2D(d);
    pathCache.set(d, p);
  }
  return p;
};

function rimGradient(ctx: CanvasRenderingContext2D) {
  const g = ctx.createLinearGradient(0, 0, SIZE, SIZE);
  g.addColorStop(0, "rgba(255,255,255,0.95)");
  g.addColorStop(0.42, "rgba(255,255,255,0.25)");
  g.addColorStop(0.58, "rgba(255,255,255,0.12)");
  g.addColorStop(1, "rgba(255,255,255,0.6)");
  return g;
}

interface Piece {
  path: Path2D;
  ox: number;
  oy: number;
  mag: number;
  alpha: number;
  light: boolean;
  tint: string;
  glowAlpha: number;
  innerAlpha: number;
  rimAlpha: number;
}

function drawGlass(ctx: CanvasRenderingContext2D, dpr: number, g: GlassSource | null, solid: Frame["solid"], q: Piece, glow: Frame["glow"], rim: CanvasGradient) {
  if (q.alpha <= 0.001) return;
  // Shadow: 0 8 16 .16 and 0 1 2 .12, cast by an opaque fill under the glass.
  ctx.save();
  ctx.globalAlpha = q.alpha;
  ctx.fillStyle = g ? "#000" : SOLID[solid ?? "dark"];
  ctx.shadowColor = "rgba(0,0,0,0.16)";
  ctx.shadowBlur = 16;
  ctx.shadowOffsetY = 8;
  ctx.fill(q.path);
  ctx.shadowColor = "rgba(0,0,0,0.12)";
  ctx.shadowBlur = 2;
  ctx.shadowOffsetY = 1;
  ctx.fill(q.path);
  ctx.restore();

  ctx.save();
  ctx.clip(q.path);
  if (g) {
    ctx.translate(q.ox, q.oy);
    ctx.scale(q.mag, q.mag);
    ctx.translate(-q.ox, -q.oy);
    ctx.drawImage(g.raw, 0, 0, SIZE, SIZE);
    ctx.globalAlpha = q.alpha; // cross-fade raw → blurred (deviation 6)
    ctx.drawImage(q.light ? g.light : g.dark, 0, 0, SIZE, SIZE);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.globalAlpha = 1;
  } else {
    ctx.globalAlpha = q.alpha;
    ctx.fillStyle = SOLID[solid ?? "dark"];
    ctx.fill(q.path);
    ctx.globalAlpha = 1;
  }
  ctx.fillStyle = q.tint;
  ctx.fill(q.path);
  if (glow && q.glowAlpha > 0.001) {
    const rg = ctx.createRadialGradient(glow.x, glow.y, 0, glow.x, glow.y, 80);
    rg.addColorStop(0, "rgba(255,255,255,0.45)");
    rg.addColorStop(1, "rgba(255,255,255,0)");
    ctx.globalAlpha = q.glowAlpha;
    ctx.fillStyle = rg;
    ctx.fill(q.path);
    ctx.globalAlpha = 1;
  }
  // Inner light: clipped 7 px stroke, softened with shadowBlur (deviation 5).
  ctx.lineWidth = 7;
  ctx.strokeStyle = `rgba(255,255,255,${q.innerAlpha.toFixed(3)})`;
  ctx.shadowColor = `rgba(255,255,255,${q.innerAlpha.toFixed(3)})`;
  ctx.shadowBlur = 4.4;
  ctx.stroke(q.path);
  ctx.restore();

  ctx.save();
  ctx.globalAlpha = q.rimAlpha * (q.light ? 0.7 : 1);
  ctx.lineWidth = 1.2;
  ctx.strokeStyle = rim;
  ctx.stroke(q.path);
  if (!g && solid === "light") {
    ctx.lineWidth = 0.5;
    ctx.strokeStyle = "rgba(0,0,0,0.12)";
    ctx.stroke(q.path);
  }
  ctx.restore();
}

function drawIcon(ctx: CanvasRenderingContext2D, name: string, x: number, y: number, size: number, color: string) {
  ctx.save();
  ctx.translate(x - size / 2, y - size / 2);
  ctx.scale(size / 24, size / 24);
  ctx.lineWidth = 1.75;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.strokeStyle = color;
  ctx.stroke(P(iconPath(name)));
  ctx.restore();
}

function slotIcon(s: SlotFrame): { icon: string; label: string } | null {
  switch (s.slot.kind) {
    case "preset":
      return { icon: s.slot.preset.icon, label: s.slot.preset.label };
    case "more":
      return { icon: "more", label: "More" };
    case "back":
      return { icon: "back", label: "Back" };
    default:
      return null;
  }
}

function drawSymbol(ctx: CanvasRenderingContext2D, s: SlotFrame) {
  const what = slotIcon(s);
  if (!what || s.symbolAlpha <= 0.001) return;
  ctx.save();
  ctx.globalAlpha = s.symbolAlpha;
  ctx.translate(s.sx, s.sy);
  ctx.scale(s.symbolScale, s.symbolScale);
  if (s.symbolColor === "#fff") {
    ctx.shadowColor = "rgba(0,0,0,0.3)";
    ctx.shadowBlur = 2;
    ctx.shadowOffsetY = 1;
  }
  drawIcon(ctx, what.icon, 0, -8, 23, s.symbolColor);
  ctx.fillStyle = s.symbolColor;
  ctx.font = '600 11px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(what.label, 0, 15);
  ctx.restore();
  if (s.badge) {
    const ca = slotAngle(s.index);
    const bx = s.sx + (TH / 2 - 12) * Math.cos(ca) + 18 * Math.cos(ca + Math.PI / 2);
    const by = s.sy + (TH / 2 - 12) * Math.sin(ca) + 18 * Math.sin(ca + Math.PI / 2);
    ctx.save();
    ctx.globalAlpha = s.symbolAlpha;
    ctx.fillStyle = s.badge === "warning" ? "rgba(255,159,10,0.95)" : `rgba(${BLUE},0.95)`;
    ctx.beginPath();
    ctx.arc(bx, by, 8, 0, Math.PI * 2);
    ctx.fill();
    drawIcon(ctx, s.badge === "warning" ? "warning" : "download", bx, by, 11, "#fff");
    ctx.restore();
  }
}

/** Paint a frame. The canvas backing store is SIZE·dpr square. */
export function draw(ctx: CanvasRenderingContext2D, frame: Frame, glass: GlassSource | null, dpr: number, fallbackKind: string | null) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const rim = rimGradient(ctx);
  const d = frame.disc;
  drawGlass(ctx, dpr, glass, frame.solid, {
    path: P(circlePath(CX, CY, d.r)), ox: CX, oy: CY, mag: d.mag, alpha: d.m, light: d.light,
    tint: d.light ? `rgba(255,255,255,${(0.35 * d.m).toFixed(3)})` : "rgba(255,255,255,0)",
    glowAlpha: 0, innerAlpha: 0.3 * d.m, rimAlpha: d.m,
  }, null, rim);
  for (const s of frame.slots) {
    drawGlass(ctx, dpr, glass, frame.solid, {
      path: P(s.path), ox: s.sx, oy: s.sy, mag: s.mag, alpha: s.glassAlpha, light: s.light,
      tint: s.tint, glowAlpha: s.glowAlpha, innerAlpha: s.innerAlpha, rimAlpha: s.rimAlpha,
    }, frame.glow, rim);
    if (s.focused) {
      ctx.save();
      ctx.lineWidth = 2.5;
      ctx.strokeStyle = `rgba(${BLUE},0.95)`;
      ctx.stroke(P(s.path));
      ctx.restore();
    }
    drawSymbol(ctx, s);
  }
  if (frame.ring) {
    const rr = d.r + 7;
    ctx.save();
    ctx.lineWidth = 4;
    ctx.lineCap = "round";
    ctx.strokeStyle = d.light ? "rgba(0,0,0,0.12)" : "rgba(255,255,255,0.25)";
    ctx.beginPath();
    ctx.arc(CX, CY, rr, 0, Math.PI * 2);
    ctx.stroke();
    ctx.strokeStyle = `rgb(${BLUE})`;
    ctx.beginPath();
    if (frame.ring.pct != null) {
      ctx.arc(CX, CY, rr, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.01, frame.ring.pct));
    } else {
      const a = frame.ring.spin - Math.PI / 2;
      ctx.arc(CX, CY, rr, a, a + Math.PI / 2);
    }
    ctx.stroke();
    ctx.restore();
  }
  if (fallbackKind && d.m > 0.01) {
    drawIcon(ctx, kindIcon(fallbackKind), CX, CY - 22, 34, d.light ? "#1d1d1f" : "#fff");
  }
}

/** Development check of the 4 ms budget (spec §4.4): ?perf in the pad URL logs mean draw time. */
export function perfProbe(enabled: boolean) {
  let total = 0, n = 0, last = performance.now();
  return <T>(fn: () => T): T => {
    if (!enabled) return fn();
    const t0 = performance.now();
    const out = fn();
    total += performance.now() - t0;
    n++;
    if (t0 - last > 2000) {
      console.debug(`[pad] mean draw ${(total / n).toFixed(2)} ms over ${n} frames`);
      total = 0;
      n = 0;
      last = t0;
    }
    return out;
  };
}
```

The type glyph for the disc is drawn on canvas only when there's no thumbnail. `Ring.tsx` passes `fallbackKind = null` once a thumbnail has loaded.

- [ ] **Step 8: Verify and commit**

```bash
npm test
npm run build
```

Expected: all pass, and tsc is clean.

```bash
git add -A
git commit -m "feat(pad): frame model, icons, backdrop preparation, Canvas 2D glass renderer"
```

**→ Opus checkpoint review 2 (Tasks 8–13) before Group D.**

---
### Task 14: Disc text, thumbnails, the `usePad` hook and the `Ring` component

**Files:**
- Create: `src/pad/disc.ts`, `src/pad/disc.test.ts`, `src/shared/useThumb.ts`, `src/pad/usePad.ts`, `src/pad/Ring.tsx`
- Modify: `src/pad/pad.css`

- [ ] **Step 1: Disc text (test first)**

`src/pad/disc.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { FileInfo, PadModel } from "../shared/types";
import { discText, formatDuration } from "./disc";

const file = (over: Partial<FileInfo> = {}): FileInfo => ({
  path: "C:\\v\\holiday.mov", name: "holiday.mov", kind: "video", ext: "mov", format: "mov",
  size: 1288490189, duration_s: 134, has_audio: true, pages: null, ...over,
});
const model = (files: FileInfo[], presets = 1, open_in_recast = false): PadModel => ({
  files, open_in_recast,
  presets: Array.from({ length: presets }, () => ({
    id: "x", label: "X", description: "x", icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
    engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
  })),
});

describe("disc text (§4.2)", () => {
  it("formats durations", () => {
    expect(formatDuration(134)).toBe("2:14");
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(3725)).toBe("1:02:05");
  });

  it("names one file with kind, size and length or pages", () => {
    expect(discText(model([file()]))).toEqual({ title: "holiday.mov", meta: "MOV · 1.2 GB · 2:14", note: null });
    expect(discText(model([file({ name: "a.pdf", kind: "pdf", format: "pdf", size: 2048, duration_s: null, pages: 3 })])).meta).toBe("PDF · 2.0 KB · 3 pages");
  });

  it("counts several files", () => {
    expect(discText(model([file(), file({ size: 10 })])).title).toBe("2 files");
  });

  it("explains an empty ring", () => {
    expect(discText(model([], 0)).note).toBe("Folders aren't supported yet. Drop the files inside instead.");
    expect(discText(model([file(), file({ kind: "pdf" })], 0, true)).note).toBe("These files are different kinds. Drop each kind separately.");
    expect(discText(model([file()], 0)).note).toBe("Recast has no conversions for this file.");
  });
});
```

`src/pad/disc.ts`:

```ts
import { formatBytes } from "../shared/format";
import type { PadModel } from "../shared/types";

export function formatDuration(s: number): string {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Centre-disc text: name, meta (e.g. "MOV · 1.2 GB · 2:14") and an explanation when nothing applies. */
export function discText(model: PadModel): { title: string; meta: string; note: string | null } {
  const { files } = model;
  const one = files.length === 1 ? files[0] : null;
  const title = one ? one.name : files.length === 0 ? "Nothing to convert" : `${files.length} files`;
  const meta = one
    ? [
        one.format.toUpperCase(),
        formatBytes(one.size),
        one.duration_s != null ? formatDuration(one.duration_s) : null,
        one.pages ? `${one.pages} pages` : null,
      ].filter(Boolean).join(" · ")
    : files.length > 0
      ? `${formatBytes(files.reduce((a, f) => a + f.size, 0))} in total`
      : "";
  const note = model.presets.length > 0
    ? null
    : files.length === 0
      ? "Folders aren't supported yet. Drop the files inside instead."
      : model.open_in_recast
        ? "These files are different kinds. Drop each kind separately."
        : "Recast has no conversions for this file.";
  return { title, meta, note };
}
```

Run: `npm test -- disc`. Expected: PASS.

- [ ] **Step 2: Thumbnail hook**

`src/shared/useThumb.ts`:

```ts
import { useEffect, useState } from "react";
import { ipc } from "./ipc";

/** Object URL of a file's thumbnail, or null (show the type glyph). Revoked on change/unmount. */
export function useThumb(path: string | null): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    setUrl(null);
    if (!path) return;
    let live = true;
    let made: string | null = null;
    ipc.thumbnail(path).then(
      (buf) => {
        if (!live || buf.byteLength === 0) return;
        made = URL.createObjectURL(new Blob([buf]));
        setUrl(made);
      },
      () => {},
    );
    return () => {
      live = false;
      if (made) URL.revokeObjectURL(made);
    };
  }, [path]);
  return url;
}
```

- [ ] **Step 3: `usePad`**

`src/pad/usePad.ts`:

```ts
/** The pad's state machine wired to IPC: starting jobs, following them, ticking timers. */
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { ipc, onJobEvent } from "../shared/ipc";
import type { PadModel, PresetView } from "../shared/types";
import { paginate } from "./paging";
import { emptyBatch, view, withEvent, withIds, type Batch, type BatchView } from "./progress";
import { closed, reduce, type PadEvent, type PadState } from "./states";

export interface Pad {
  state: PadState;
  dispatch: (e: PadEvent) => void;
  batch: BatchView;
  /** The preset being run (for the engine download label). */
  running: PresetView | null;
  openedAt: number;
  open: (model: PadModel, paths: string[]) => void;
}

export function usePad(opts: { reducedMotion: boolean; onClosed: () => void }): Pad {
  const [state, dispatch] = useReducer(reduce, closed);
  const [batch, setBatch] = useState<Batch>(emptyBatch);
  const [openedAt, setOpenedAt] = useState(0);
  const paths = useRef<string[]>([]);
  const model = useRef<PadModel | null>(null);
  const following = useRef(false);
  const onClosed = useRef(opts.onClosed);
  onClosed.current = opts.onClosed;
  const wasOpen = useRef(false);

  const open = useCallback(
    (m: PadModel, p: string[]) => {
      model.current = m;
      paths.current = p;
      following.current = false;
      setBatch(emptyBatch());
      const at = performance.now();
      setOpenedAt(at);
      dispatch({ type: "open", pages: paginate(m.presets), at, reducedMotion: opts.reducedMotion });
    },
    [opts.reducedMotion],
  );

  // Starting: events can arrive before start_jobs returns, so follow from now on.
  const phase = state.phase;
  useEffect(() => {
    if (phase.name !== "starting") return;
    following.current = true;
    let live = true;
    ipc.startJobs(phase.presetId, paths.current).then(
      (ids) => {
        if (!live) return;
        setBatch((b) => withIds(b, ids));
        dispatch({ type: "started" });
      },
      (e) => live && dispatch({ type: "startFailed", message: String(e), at: performance.now() }),
    );
    return () => {
      live = false;
    };
  }, [phase]);

  useEffect(() => {
    const off = onJobEvent((e) => {
      if (following.current) setBatch((b) => withEvent(b, e));
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  const v = view(batch);
  useEffect(() => {
    if (phase.name === "running" && v.settled) {
      following.current = false;
      dispatch({ type: "settled", failed: v.failed, at: performance.now() });
    }
  }, [phase.name, v.settled, v.failed]);

  // Timers (open → idle, dwell, leave, done/failed) run off a 50 ms tick while open.
  const isOpen = phase.name !== "closed";
  useEffect(() => {
    if (!isOpen) return;
    const t = window.setInterval(() => dispatch({ type: "tick", at: performance.now() }), 50);
    return () => window.clearInterval(t);
  }, [isOpen]);

  useEffect(() => {
    if (isOpen) wasOpen.current = true;
    else if (wasOpen.current) {
      wasOpen.current = false;
      following.current = false;
      onClosed.current();
    }
  }, [isOpen]);

  const running =
    ["starting", "running", "done", "failed"].includes(phase.name)
      ? (model.current?.presets.find((p) => p.id === runningId.current) ?? null)
      : null;

  return { state, dispatch, batch: v, running, openedAt, open };
}
```

In the starting effect, record the preset before calling `startJobs`: declare `const runningId = useRef<string | null>(null);` next to the other refs and set `runningId.current = phase.presetId;` as the effect's first line after the `starting` guard.

- [ ] **Step 4: `Ring.tsx`**

```tsx
/** The ring: one canvas for the glass, a small DOM overlay for the disc text and pill. */
import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../shared/format";
import type { PadModel } from "../shared/types";
import { buildFrame, restSprings, springsSettled, stepRetract, stepSprings, type Spring } from "./frame";
import { OPEN_MS, PAGE_FADE_MS } from "./anim";
import { CX, CY, R1, SIZE, slotAngle } from "./geometry";
import { hitTest } from "./hitTest";
import { discText } from "./disc";
import { label } from "./progress";
import { currentPage, target, type PadKey } from "./states";
import type { Pad } from "./usePad";
import { allSlotLuminance, discLuminance } from "./luminance";
import type { GlassSource } from "./glass/backdrop";
import { draw, perfProbe } from "./glass/renderer";

export interface RingProps {
  pad: Pad;
  model: PadModel;
  thumbUrl: string | null;
  glass: GlassSource | null;
  /** Surface for the solid look when there's no glass. */
  solid: "dark" | "light";
  /** Pointer from a drag over the window (logical px), when the mouse isn't reporting. */
  external: { x: number; y: number } | null;
  /** Listen for keys on the window (the pad window, or the inline overlay while open). */
  keys: boolean;
  /** "Escape"/click-outside are handled by the host; Ring reports clicks inside. */
  className?: string;
}

const KEYS: Record<string, PadKey> = { ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight", Enter: "Enter", Escape: "Escape", Tab: "Tab" };

export function Ring({ pad, model, thumbUrl, glass, solid, external, keys, className }: RingProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const { state, dispatch, batch, running, openedAt } = pad;
  const tgt = target(state);
  const page = currentPage(state);

  // Luminance once per backdrop.
  const lum = useRef<{ slots: number[]; disc: number } | null>(null);
  useEffect(() => {
    lum.current = glass ? { slots: allSlotLuminance(glass.pixels, glass.scale), disc: discLuminance(glass.pixels, glass.scale) } : null;
  }, [glass]);

  // Pointer → hit test (mouse here, or an external drag position).
  const pos = external ?? pointer;
  useEffect(() => {
    dispatch({ type: "pointer", hit: pos ? hitTest(pos.x, pos.y, tgt) : { kind: "none" }, at: performance.now() });
    // tgt is deliberately not a dependency: it would re-dispatch on every target change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pos?.x, pos?.y, dispatch]);

  useEffect(() => {
    if (!keys) return;
    const onKey = (e: KeyboardEvent) => {
      const k = e.key === "Tab" && e.shiftKey ? "ShiftTab" : KEYS[e.key];
      if (!k) return;
      e.preventDefault();
      dispatch({ type: "key", key: k, at: performance.now() });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keys, dispatch]);

  // The render loop: draws only while something moves.
  const latest = useRef({ state, glass, solid, pos, batch, thumbUrl });
  latest.current = { state, glass, solid, pos, batch, thumbUrl };
  const dirty = useRef(true);
  useEffect(() => {
    dirty.current = true;
  }, [state, glass, solid, pos, batch, thumbUrl]);
  useEffect(() => {
    const c = canvas.current!;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(SIZE * dpr);
    c.height = Math.round(SIZE * dpr);
    const ctx = c.getContext("2d")!;
    let springs: Spring[] = restSprings();
    let retract = 0;
    let last = performance.now();
    let raf = 0;
    const probe = perfProbe(new URLSearchParams(location.search).has("perf"));
    const loop = (now: number) => {
      const dt = Math.min(64, now - last);
      last = now;
      const L = latest.current;
      const s = L.state;
      const t = target(s);
      const busy = ["starting", "running", "done", "failed"].includes(s.phase.name);
      springs = stepSprings(springs, busy ? null : t, dt, s.reducedMotion);
      const nextRetract = stepRetract(retract, busy, dt, s.reducedMotion);
      const moving =
        dirty.current ||
        !springsSettled(springs, busy ? null : t) ||
        Math.abs(nextRetract - retract) > 0.001 ||
        now - openedAt < OPEN_MS ||
        now - s.pageAt < PAGE_FADE_MS ||
        (busy && L.batch.pct == null && !s.reducedMotion);
      retract = nextRetract;
      if (moving) {
        dirty.current = false;
        const frame = buildFrame({
          now, openedAt, page: currentPage(s), pageAt: s.pageAt, target: busy ? null : t, focus: s.focus,
          showFocus: s.keyboard, springs, lum: lum.current?.slots ?? null, discLum: lum.current?.disc ?? null,
          solid: L.glass ? null : L.solid, reducedMotion: s.reducedMotion, pointer: L.pos, retract,
          progress: busy ? { pct: L.batch.pct, spin: s.reducedMotion ? 0 : (now / 1000) * Math.PI * 1.6 } : null,
        });
        probe(() => draw(ctx, frame, L.glass, dpr, L.thumbUrl ? null : (model.files[0]?.kind ?? "unknown")));
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [openedAt, model]);

  const local = (e: React.MouseEvent) => {
    const r = box.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const text = discText(model);
  const busy = ["starting", "running"].includes(state.phase.name);
  const light = lum.current ? lum.current.disc > 0.6 : solid === "light";
  const tslot = tgt != null ? page[tgt] : null;
  const pill =
    tslot?.kind === "preset"
      ? [
          tslot.preset.description,
          tslot.preset.engine_status.state !== "installed" ? `Downloads ${tslot.preset.engine_name} · ${formatBytes(tslot.preset.engine_size)}` : null,
          tslot.preset.warning,
        ].filter(Boolean) as string[]
      : tslot?.kind === "more" ? ["More conversions"] : tslot?.kind === "back" ? ["Back"] : null;
  const pa = tgt != null ? slotAngle(tgt) : 0;
  const pillX = Math.min(SIZE - 90, Math.max(90, CX + (R1 + 26) * Math.cos(pa)));
  const pillY = Math.min(SIZE - 24, Math.max(24, CY + (R1 + 26) * Math.sin(pa)));
  const status =
    state.phase.name === "done" ? "Done"
      : state.phase.name === "failed" ? state.phase.message
        : busy ? label(batch, running) : null;

  return (
    <div
      ref={box}
      className={`ring ${light ? "on-light" : ""} ${className ?? ""}`}
      onMouseMove={(e) => setPointer(local(e))}
      onMouseLeave={() => setPointer(null)}
      onClick={(e) => {
        const p = local(e);
        dispatch({ type: "drop", hit: hitTest(p.x, p.y, tgt), at: performance.now() });
      }}
    >
      <canvas ref={canvas} className="ring-canvas" aria-hidden="true" />
      <div className="disc" aria-live="polite">
        {state.phase.name === "done" ? (
          <div className="disc-glyph ok" aria-hidden="true">✓</div>
        ) : state.phase.name === "failed" ? (
          <div className="disc-glyph bad" aria-hidden="true">⚠</div>
        ) : thumbUrl ? (
          <div className={model.files.length > 1 ? "thumb stacked" : "thumb"}>
            <img src={thumbUrl} alt="" />
          </div>
        ) : (
          <div className="thumb-space" />
        )}
        <div className="disc-title">{text.title}</div>
        <div className="disc-meta">{status ?? text.note ?? text.meta}</div>
      </div>
      {pill && !busy && (
        <div className="pill" style={{ left: pillX, top: pillY }} role="tooltip">
          {pill.map((l) => (
            <div key={l}>{l}</div>
          ))}
        </div>
      )}
      <ul className="sr-only" role="listbox" aria-label="Conversions">
        {page.map((s, i) =>
          s.kind === "empty" ? null : (
            <li key={i} role="option" aria-selected={state.focus === i}>
              {s.kind === "preset" ? s.preset.description : s.kind === "more" ? "More conversions" : "Back"}
            </li>
          ),
        )}
      </ul>
    </div>
  );
}
```

If eslint isn't set up in this repo (there's no `.eslintrc`), drop the `eslint-disable` comment and keep the explanatory comment.

- [ ] **Step 5: Styles**

Append to `src/pad/pad.css`. The ring styles are shared, and the companion imports this file in Task 16.

```css
.ring { position: relative; width: 440px; height: 440px; color: #fff; cursor: default; }
.ring-canvas { position: absolute; inset: 0; width: 440px; height: 440px; }
.ring .disc {
  position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
  width: 160px; display: flex; flex-direction: column; align-items: center; text-align: center;
  pointer-events: none; text-shadow: 0 1px 2px rgba(0,0,0,.35);
}
.ring.on-light .disc { color: #1d1d1f; text-shadow: none; }
.ring .thumb { width: 88px; height: 58px; border-radius: 10px; overflow: hidden;
  box-shadow: 0 8px 20px rgba(0,0,0,.3), 0 0 0 .5px rgba(255,255,255,.6); background: rgba(0,0,0,.2); }
.ring .thumb.stacked { box-shadow: 4px -4px 0 -1px rgba(255,255,255,.35), 8px -8px 0 -2px rgba(255,255,255,.2), 0 8px 20px rgba(0,0,0,.3); }
.ring .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
.ring .thumb-space { height: 44px; }
.ring .disc-glyph { font-size: 34px; line-height: 58px; height: 58px; }
.ring .disc-glyph.ok { color: #30d158; }
.ring .disc-glyph.bad { color: #ff9f0a; }
.ring .disc-title { margin-top: 8px; font-size: 12px; font-weight: 600; max-width: 150px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ring .disc-meta { margin-top: 2px; font-size: 10.5px; font-weight: 400; opacity: .85; max-width: 150px; }
.ring .pill {
  position: absolute; transform: translate(-50%, -50%); padding: 6px 12px; border-radius: 999px;
  font: 600 12px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif; color: #fff; white-space: nowrap;
  background: rgba(30,30,36,.88); pointer-events: none; text-align: center;
  box-shadow: inset 0 1px 0 rgba(255,255,255,.35), 0 0 0 .5px rgba(255,255,255,.2), 0 8px 20px rgba(0,0,0,.25);
}
.ring .pill div + div { font-weight: 400; font-size: 11px; opacity: .85; }
.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
```

- [ ] **Step 6: Verify and commit**

```bash
npm test
npm run build
```

```bash
git add -A
git commit -m "feat(pad): Ring component, usePad hook, disc text and thumbnails"
```

---

### Task 15: The pad window app

**Files:**
- Modify: `src/pad/PadApp.tsx`

- [ ] **Step 1: Implement**

Replace `src/pad/PadApp.tsx`:

```tsx
/** The pad window (spec §4.2–4.4, §6.8): backdrop, thumbnail, drag targeting. */
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc, onPadCursor, onPadOpen } from "../shared/ipc";
import type { PadMode, PadModel, PadOpen } from "../shared/types";
import { useThumb } from "../shared/useThumb";
import { prepare, release, type GlassSource } from "./glass/backdrop";
import { SIZE } from "./geometry";
import { hitTest } from "./hitTest";
import { Ring } from "./Ring";
import { target } from "./states";
import { usePad } from "./usePad";

interface Session {
  key: number;
  paths: string[];
  mode: PadMode;
  model: PadModel;
  glass: GlassSource | null;
}

const reducedMotionQuery = () => window.matchMedia("(prefers-reduced-motion: reduce)");
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");
const samePaths = (a: string[], b: string[]) => a.length === b.length && a.every((p, i) => p === b[i]);
const EMPTY: PadModel = { files: [], presets: [], open_in_recast: false };

export function PadApp() {
  const [session, setSession] = useState<Session | null>(null);
  const [external, setExternal] = useState<{ x: number; y: number } | null>(null);
  const [reducedMotion, setReducedMotion] = useState(reducedMotionQuery().matches);
  const seq = useRef(0);
  const sessionRef = useRef<Session | null>(null);
  sessionRef.current = session;
  const dragging = useRef(false);

  useEffect(() => {
    const q = reducedMotionQuery();
    const on = () => setReducedMotion(q.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, []);

  const finish = useCallback(() => {
    release(sessionRef.current?.glass ?? null);
    dragging.current = false;
    setExternal(null);
    setSession(null);
    ipc.padTrackCursor(false).catch(() => {});
    ipc.padClose().catch(() => {});
  }, []);

  const pad = usePad({ reducedMotion, onClosed: finish });
  const padRef = useRef(pad);
  padRef.current = pad;

  const begin = useCallback(async (req: PadOpen) => {
    const key = ++seq.current;
    const [model, glass] = await Promise.all([
      ipc.inspect(req.paths).catch((e) => {
        console.warn("inspect failed", e);
        return EMPTY;
      }),
      req.reduced_transparency
        ? Promise.resolve(null)
        : ipc.captureBackdrop().then(prepare).catch((e) => {
            console.warn("backdrop unavailable, using the solid look", e);
            return null;
          }),
    ]);
    if (key !== seq.current) {
      release(glass);
      return;
    }
    release(sessionRef.current?.glass ?? null);
    setSession({ key, paths: req.paths, mode: req.mode, model, glass });
    padRef.current.open(model, req.paths);
  }, []);

  // Listen first, then tell Rust we're ready (it may be holding an open for us).
  useEffect(() => {
    const offs = [
      onPadOpen((req) => void begin(req)),
      onPadCursor((p) => {
        const dx = Math.max(0, -p.x, p.x - SIZE);
        const dy = Math.max(0, -p.y, p.y - SIZE);
        padRef.current.dispatch({ type: "outside", distance: Math.hypot(dx, dy), at: performance.now() });
      }),
    ];
    Promise.all(offs).then(() => ipc.padReady());
    return () => offs.forEach((o) => o.then((f) => f()));
  }, [begin]);

  // Drags over the pad: Over positions drive targeting; Drop runs the targeted segment.
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const scale = window.devicePixelRatio || 1;
      if (p.type === "enter") {
        dragging.current = true;
        ipc.padTrackCursor(true).catch(() => {});
      } else if (p.type === "over") {
        setExternal({ x: p.position.x / scale, y: p.position.y / scale });
      } else if (p.type === "leave") {
        setExternal(null);
      } else if (p.type === "drop") {
        dragging.current = false;
        setExternal(null);
        ipc.padTrackCursor(false).catch(() => {});
        const s = sessionRef.current;
        const x = p.position.x / scale, y = p.position.y / scale;
        if (s && samePaths(p.paths, s.paths)) {
          padRef.current.dispatch({ type: "drop", hit: hitTest(x, y, target(padRef.current.state)), at: performance.now() });
        } else if (p.paths.length > 0) {
          void begin({ paths: p.paths, mode: "click", reduced_transparency: !s?.glass });
        }
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [begin]);

  // Click mode: clicking anywhere else (the pad loses focus) dismisses it (§4.3).
  useEffect(() => {
    const off = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused && sessionRef.current?.mode === "click" && !dragging.current) {
        padRef.current.dispatch({ type: "dismiss" });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  const thumb = useThumb(session?.model.files[0]?.path ?? null);
  if (!session) return null;
  return (
    <Ring
      key={session.key}
      pad={pad}
      model={session.model}
      thumbUrl={thumb}
      glass={session.glass}
      solid={darkQuery().matches ? "dark" : "light"}
      external={external}
      keys
    />
  );
}
```

Notes for the implementer:
- `p.position` in Tauri 2's drag-drop payload is a `PhysicalPosition` relative to the webview. Check `node_modules/@tauri-apps/api/webview.d.ts` and confirm `enter`, `over` and `drop` carry `position`, and that only `enter`/`drop` carry `paths`. Adapt the field access, keeping the behaviour.
- A click outside the segments arrives as a `drop` with hit `none`, and the machine closes the pad. A click on the disc does nothing.
- When the session closes, `finish` hides the window and the Ring unmounts, so the next open never flashes the previous frame.

- [ ] **Step 2: Verify and commit**

```bash
npm test
npm run build
```

```bash
git add -A
git commit -m "feat(pad): pad window app (backdrop, drag targeting, click-outside dismiss)"
```

---

### Task 16: The ring inline in Activity, and one drag router

**Files:**
- Create: `src/companion/dragRouter.ts`, `src/companion/dragRouter.test.ts`, `src/companion/InlineRing.tsx`
- Modify: `src/companion/App.tsx`, `src/companion/Activity.tsx`, `src/companion/companion.css`, `src/main.tsx`

- [ ] **Step 1: Router (test first)**

`src/companion/dragRouter.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { createRouter, type DragEv } from "./dragRouter";

describe("drag router", () => {
  it("sends events to the active section's handler only", () => {
    const r = createRouter();
    const a = vi.fn(), t = vi.fn();
    r.subscribe("activity", a);
    const off = r.subscribe("tools", t);
    const drop: DragEv = { type: "drop", paths: ["C:\\a.pdf"] };
    r.route("tools", drop);
    expect(t).toHaveBeenCalledWith(drop);
    expect(a).not.toHaveBeenCalled();
    off();
    r.route("tools", drop);
    expect(t).toHaveBeenCalledTimes(1);
  });

  it("drops anywhere but Tools go to Activity", () => {
    expect(createRouter().targetFor("library")).toBe("activity");
    expect(createRouter().targetFor("settings")).toBe("activity");
    expect(createRouter().targetFor("tools")).toBe("tools");
  });
});
```

`src/companion/dragRouter.ts`:

```ts
/** One drag-drop listener for the companion, routed to the section that should get it. */
export type DragEv = { type: "enter" | "over" | "leave" } | { type: "drop"; paths: string[] };
export type DragTarget = "activity" | "tools";
type Handler = (e: DragEv) => void;

export function createRouter() {
  const handlers = new Map<DragTarget, Handler>();
  return {
    subscribe(t: DragTarget, h: Handler) {
      handlers.set(t, h);
      return () => {
        if (handlers.get(t) === h) handlers.delete(t);
      };
    },
    route(t: DragTarget, e: DragEv) {
      handlers.get(t)?.(e);
    },
    targetFor(section: string): DragTarget {
      return section === "tools" ? "tools" : "activity";
    },
  };
}

export const router = createRouter();
```

Run `npm test -- dragRouter` and expect PASS.

- [ ] **Step 2: App owns the listener and gains Tools**

In `src/companion/App.tsx`:
- `SECTIONS` becomes Activity, Tools, Library, Settings (Tools second).
- Add a `<main className="content" hidden={section !== "tools"}><Tools /></main>` panel (`Tools` comes in Task 17). Create a stub file `src/companion/Tools.tsx` exporting `export function Tools() { return <section aria-label="Tools" />; }` so this task builds.
- `Activity` no longer takes `onDrop`.
- Add one listener:

```tsx
  const sectionRef = useRef(section);
  sectionRef.current = section;
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const t = router.targetFor(sectionRef.current);
      if (p.type === "drop") {
        if (t === "activity") setSection("activity"); // the ring lives in Activity
        router.route(t, { type: "drop", paths: p.paths });
      } else {
        router.route(t, { type: p.type });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);
```

(import `useEffect`, `useRef`, `getCurrentWebview`, `router`, `Tools`).

In `src/main.tsx` add `import "./pad/pad.css";` after the companion CSS so the ring styles load. Then remove the pad-window-only `html, body.pad-body…` rule from the shared stylesheet by splitting it:
- Move that first rule into a new `src/pad/padWindow.css` that only `src/pad/main.tsx` imports (alongside `pad.css`).
- `pad.css` keeps only the `.ring…`/`.pill`/`.sr-only` rules.

- [ ] **Step 3: InlineRing**

`src/companion/InlineRing.tsx`:

```tsx
/** The ring inside the companion (spec §4.5 item 1): solid look, click or keyboard. */
import { useEffect, useState } from "react";
import type { PadModel } from "../shared/types";
import { useThumb } from "../shared/useThumb";
import { Ring } from "../pad/Ring";
import { usePad } from "../pad/usePad";

const mq = (q: string) => window.matchMedia(q);

export function InlineRing({ paths, model, onClose }: { paths: string[]; model: PadModel; onClose: () => void }) {
  const [reducedMotion] = useState(() => mq("(prefers-reduced-motion: reduce)").matches);
  const pad = usePad({ reducedMotion, onClosed: onClose });
  const { open } = pad;
  useEffect(() => {
    open(model, paths);
  }, [model, paths, open]);
  const thumb = useThumb(model.files[0]?.path ?? null);
  return (
    <div
      className="ring-overlay"
      role="dialog"
      aria-label="Choose a conversion"
      onClick={(e) => {
        if (e.target === e.currentTarget) pad.dispatch({ type: "dismiss" });
      }}
    >
      <Ring pad={pad} model={model} thumbUrl={thumb} glass={null}
        solid={mq("(prefers-color-scheme: dark)").matches ? "dark" : "light"} external={null} keys />
    </div>
  );
}
```

`companion.css`: add

```css
.ring-overlay { position: fixed; inset: 0; display: grid; place-items: center; background: rgba(0,0,0,.28); z-index: 10; }
@media (prefers-color-scheme: light) { .ring-overlay .ring { color: #1d1d1f; } .ring-overlay .ring .disc { text-shadow: none; } }
```

and delete the now-unused `.picker` / `.presets` rules (keep `.badge` if Library uses it; grep first).

- [ ] **Step 4: Activity uses the router and the inline ring**

In `Activity.tsx`:
- Remove the `getCurrentWebview` effect, the `onDrop` prop, `PresetPicker` and its `starting`/`start` state.
- Subscribe to the router:

```tsx
  useEffect(
    () =>
      router.subscribe("activity", async (ev) => {
        if (ev.type === "enter" || ev.type === "over") setHovering(true);
        else if (ev.type === "leave") setHovering(false);
        else {
          setHovering(false);
          if (ev.paths.length === 0) return;
          try {
            setError(null);
            setDropped({ paths: ev.paths, pad: await ipc.inspect(ev.paths) });
          } catch (e) {
            setError(String(e));
          }
        }
      }),
    [],
  );
```

- Render `{dropped && <InlineRing paths={dropped.paths} model={dropped.pad} onClose={() => setDropped(null)} />}` where the picker was.
- The ring starts the job itself. Activity shows it through the job events it already receives, so nothing else changes.

`jobs.ts` and its tests are unchanged.

- [ ] **Step 5: Verify and commit**

```bash
npm test
npm run build
```

```bash
git add -A
git commit -m "feat(companion): inline ring replaces the preset list; one routed drag listener; Tools section"
```

---

### Task 17: The Tools page

**Files:**
- Create: `src/companion/toolList.ts`, `src/companion/toolList.test.ts`, `src/companion/SortableList.tsx`
- Modify: `src/companion/Tools.tsx`, `src/companion/companion.css`

- [ ] **Step 1: List logic (test first)**

`src/companion/toolList.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { FileInfo } from "../shared/types";
import { accept, rejection } from "./toolList";

const f = (name: string, kind: FileInfo["kind"]): FileInfo => ({
  path: `C:\\d\\${name}`, name, kind, ext: name.split(".").pop()!, format: "x", size: 1, duration_s: null, has_audio: null, pages: kind === "pdf" ? 2 : null,
});
let n = 0;
const id = () => `id${n++}`;

describe("tool lists", () => {
  it("keeps the right kind in drop order and skips duplicates", () => {
    const first = accept([f("b.pdf", "pdf"), f("notes.docx", "text"), f("a.pdf", "pdf")], "pdf", [], id);
    expect(first.items.map((i) => i.name)).toEqual(["b.pdf", "a.pdf"]);
    expect(first.rejected).toEqual(["notes.docx"]);
    const again = accept([f("a.pdf", "pdf"), f("c.pdf", "pdf")], "pdf", first.items, id);
    expect(again.items.map((i) => i.name)).toEqual(["b.pdf", "a.pdf", "c.pdf"]);
    expect(again.items[0].pages).toBe(2);
  });

  it("explains what was left out", () => {
    expect(rejection(["notes.docx"], "pdf")).toBe("notes.docx isn't a PDF.");
    expect(rejection(["a.mov", "b.mov"], "image")).toBe("2 files aren't images.");
    expect(rejection([], "pdf")).toBeNull();
  });
});
```

`src/companion/toolList.ts`:

```ts
import type { FileInfo, Kind } from "../shared/types";

export interface ToolItem {
  id: string;
  path: string;
  name: string;
  size: number;
  pages: number | null;
}

/** Add files of `kind` after `existing`, in the order given, without duplicates. */
export function accept(files: FileInfo[], kind: Kind, existing: ToolItem[], makeId: () => string) {
  const items = [...existing];
  const rejected: string[] = [];
  for (const file of files) {
    if (file.kind !== kind) rejected.push(file.name);
    else if (!items.some((i) => i.path === file.path))
      items.push({ id: makeId(), path: file.path, name: file.name, size: file.size, pages: file.pages });
  }
  return { items, rejected };
}

const NOUN: Partial<Record<Kind, [string, string]>> = { pdf: ["a PDF", "PDFs"], image: ["an image", "images"] };

export function rejection(names: string[], kind: Kind): string | null {
  if (names.length === 0) return null;
  const [one, many] = NOUN[kind] ?? ["the right kind", "the right kind"];
  return names.length === 1 ? `${names[0]} isn't ${one}.` : `${names.length} files aren't ${many}.`;
}
```

Run `npm test -- toolList` and expect PASS.

- [ ] **Step 2: SortableList (pointer-event sorter)**

`src/companion/SortableList.tsx`:

```tsx
/** Reorderable list on pointer events (dnd-kit), not HTML5 drag-and-drop: Tauri's
 *  dragDropEnabled must stay on for file drops, and that disables HTML5 DnD (§4.5). */
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { formatBytes } from "../shared/format";
import type { ToolItem } from "./toolList";

function Row({ item, onRemove }: { item: ToolItem; onRemove: () => void }) {
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({ id: item.id });
  return (
    <li ref={setNodeRef} className={isDragging ? "row sortable dragging" : "row sortable"} style={{ transform: CSS.Transform.toString(transform), transition }}>
      <button ref={setActivatorNodeRef} className="handle ghost" aria-label={`Move ${item.name}`} {...attributes} {...listeners}>
        ⋮⋮
      </button>
      <div>
        <strong>{item.name}</strong>
        <div className="muted">{[formatBytes(item.size), item.pages ? `${item.pages} pages` : null].filter(Boolean).join(" · ")}</div>
      </div>
      <button className="ghost" aria-label={`Remove ${item.name}`} onClick={onRemove}>
        ✕
      </button>
    </li>
  );
}

export function SortableList({ items, onChange }: { items: ToolItem[]; onChange: (next: ToolItem[]) => void }) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const from = items.findIndex((i) => i.id === active.id);
    const to = items.findIndex((i) => i.id === over.id);
    onChange(arrayMove(items, from, to));
  };
  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
      <SortableContext items={items.map((i) => i.id)} strategy={verticalListSortingStrategy}>
        <ol className="rows tool-list">
          {items.map((item) => (
            <Row key={item.id} item={item} onRemove={() => onChange(items.filter((i) => i.id !== item.id))} />
          ))}
        </ol>
      </SortableContext>
    </DndContext>
  );
}
```

- [ ] **Step 3: Tools page**

Replace `src/companion/Tools.tsx`:

```tsx
/** Tools (spec §4.5 item 2): ordered multi-file jobs and Split by page ranges. */
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "../shared/ipc";
import { parseRanges } from "../shared/pageRanges";
import type { Kind } from "../shared/types";
import { router } from "./dragRouter";
import { SortableList } from "./SortableList";
import { accept, rejection, type ToolItem } from "./toolList";

type ToolId = "merge" | "images" | "split";
const IMAGE_EXTS = ["jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "gif"];
const TOOLS: Record<ToolId, { title: string; kind: Kind; hint: string; exts: string[] }> = {
  merge: { title: "Merge PDFs", kind: "pdf", hint: "Put the PDFs in order, then merge them into one.", exts: ["pdf"] },
  images: { title: "Images → PDF", kind: "image", hint: "Put the images in page order, then combine them into one PDF.", exts: IMAGE_EXTS },
  split: { title: "Split PDF", kind: "pdf", hint: "Choose pages, e.g. 1-3, 5, 8-. Each range becomes its own PDF. Leave it empty for one PDF per page.", exts: ["pdf"] },
};

let idSeq = 0;
const newId = () => `t${++idSeq}`;

export function Tools() {
  const [tool, setTool] = useState<ToolId>("merge");
  const [lists, setLists] = useState<Record<ToolId, ToolItem[]>>({ merge: [], images: [], split: [] });
  const [ranges, setRanges] = useState("");
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [hovering, setHovering] = useState(false);
  const t = TOOLS[tool];
  const items = lists[tool];
  const toolRef = useRef(tool);
  toolRef.current = tool;

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    const id = toolRef.current;
    try {
      const model = await ipc.inspect(paths);
      setLists((all) => {
        const base = id === "split" ? [] : all[id];
        const files = id === "split" ? model.files.filter((f) => f.kind === "pdf").slice(0, 1).concat(model.files.filter((f) => f.kind !== "pdf")) : model.files;
        const { items: next, rejected } = accept(files, TOOLS[id].kind, base, newId);
        const msg = rejection(rejected, TOOLS[id].kind);
        setNote(msg ? { text: msg, error: true } : null);
        return { ...all, [id]: next };
      });
    } catch (e) {
      setNote({ text: String(e), error: true });
    }
  }

  useEffect(
    () =>
      router.subscribe("tools", (ev) => {
        if (ev.type === "enter" || ev.type === "over") setHovering(true);
        else if (ev.type === "leave") setHovering(false);
        else {
          setHovering(false);
          void add(ev.paths);
        }
      }),
    [],
  );

  async function browse() {
    const picked = await open({ multiple: tool !== "split", filters: [{ name: t.title, extensions: t.exts }] });
    const paths = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
    await add(paths);
  }

  const split = tool === "split";
  const pdf = split ? items[0] : null;
  const check = split && ranges.trim() !== "" ? parseRanges(ranges, pdf?.pages ?? null) : null;
  const canRun = !busy && (split ? !!pdf && (check == null || check.ok) : items.length >= 2);

  async function run() {
    setBusy(true);
    try {
      if (split && pdf) {
        if (ranges.trim() === "") await ipc.startJobs("pdf-split", [pdf.path]);
        else await ipc.startJobs("pdf-extract", [pdf.path], { ranges });
      } else {
        await ipc.startJobs(tool === "merge" ? "pdf-merge" : "image-combine-pdf", items.map((i) => i.path));
      }
      setLists((all) => ({ ...all, [tool]: [] }));
      setRanges("");
      setNote({ text: "Started. Follow it in Activity.", error: false });
    } catch (e) {
      setNote({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={hovering ? "tools drop-hover" : "tools"} aria-label="Tools">
      <div className="segmented" role="tablist" aria-label="Tool">
        {(Object.keys(TOOLS) as ToolId[]).map((id) => (
          <button key={id} role="tab" aria-selected={tool === id} className={tool === id ? "nav active" : "nav"}
            onClick={() => { setTool(id); setNote(null); }}>
            {TOOLS[id].title}
          </button>
        ))}
      </div>
      <p className="muted">{t.hint}</p>
      <div className="dropzone">
        {hovering ? "Release to add" : split ? "Drop a PDF here, or" : "Drop files here, or"}{" "}
        <button onClick={browse}>{split ? "choose a PDF" : "add files"}</button>
      </div>
      {note && <p className={note.error ? "error" : "muted"} role={note.error ? "alert" : "status"}>{note.text}</p>}
      {!split && items.length > 0 && (
        <SortableList items={items} onChange={(next) => setLists((all) => ({ ...all, [tool]: next }))} />
      )}
      {split && pdf && (
        <div className="split">
          <div className="row">
            <div>
              <strong>{pdf.name}</strong>
              <div className="muted">{pdf.pages ? `${pdf.pages} pages` : "Page count unknown until qpdf is installed"}</div>
            </div>
          </div>
          <label className="field">
            <span>Pages</span>
            <input type="text" value={ranges} placeholder="1-3, 5, 8-" onChange={(e) => setRanges(e.target.value)}
              aria-invalid={check != null && !check.ok} aria-describedby="range-help" />
          </label>
          <p id="range-help" className={check && !check.ok ? "error" : "muted"}>
            {check && !check.ok ? check.error : check?.ok ? `${check.ranges.length} PDF${check.ranges.length > 1 ? "s" : ""}` : "Empty: one PDF per page"}
          </p>
        </div>
      )}
      <button className="run" disabled={!canRun} onClick={run}>
        {split ? "Split" : tool === "merge" ? "Merge" : "Make PDF"}
      </button>
    </section>
  );
}
```

`companion.css`:

```css
.segmented { display: flex; gap: 4px; margin-bottom: 8px; }
.tools .dropzone { padding: 16px; }
.drop-hover.tools .dropzone { border-color: var(--accent); background: rgba(10, 132, 255, 0.08); color: var(--text); }
.tool-list { margin-top: 12px; }
.row.sortable { touch-action: none; }
.row.sortable.dragging { opacity: 0.85; box-shadow: 0 8px 20px rgba(0, 0, 0, 0.18); z-index: 1; position: relative; }
.handle { cursor: grab; letter-spacing: -2px; padding: 4px 6px; }
.run { margin-top: 14px; background: var(--accent); color: #fff; border-color: transparent; font-weight: 600; padding: 6px 16px; }
.run:disabled { opacity: 0.45; cursor: default; }
```

- [ ] **Step 4: Verify and commit**

```bash
npm test
npm run build
```

```bash
git add -A
git commit -m "feat(companion): Tools page (Merge PDFs, Images → PDF with dnd-kit ordering, Split by page ranges)"
```

---

### Task 18: Verification, the running app, docs

**Files:**
- Modify: `docs/superpowers/specs/2026-11-18-recast-design.md`, `README.md`
- Create: `docs/superpowers/handoff-plan-3.md`

- [ ] **Step 1: Full automated checks**

```bash
npm test
npm run build
cd src-tauri
cargo fmt --check
cargo clippy -j 2 --all-targets -- -D warnings
cargo test -j 2
cargo test -j 2 --features engines --test engines_integration -- --test-threads=1
```

Expected: everything passes, including the new `pdf-extract` integration test. Record the counts (Rust unit, Vitest and integration tests) for the report.

- [ ] **Step 2: Drive the real app**

Run `npm run tauri dev` (allow about 20 s for Vite's first module) and check, with screenshots:
1. The companion opens with Activity, Tools, Library and Settings.
2. Dropping a file on Activity (or simulating the drop) shows the inline ring. Clicking a segment starts a job, and the ring shows the progress rim and then ✓.
3. Tray → "Convert files…" → pick a video. The pad appears at the cursor, over the desktop, as glass with a thumbnail. Hovering lifts and tints segments and shows the pill. More pages, Esc closes and a click outside closes.
4. The pad is absent from its own backdrop (no ghost ring in the glass).
5. Tools: add 3 PDFs, reorder them by dragging the handle, Merge. Split with `1, 2-`.
6. `?perf` frame time: open the pad page with `?perf` (in dev, temporarily point the pad window's `url` to `pad.html?perf`) and read `[pad] mean draw … ms` in the devtools console while hovering. Record the number. The budget is ≤ 4 ms.

Anything that can't be driven automatically goes on the manual list in Step 4. Stop only the processes you started.

- [ ] **Step 3: Installer**

```bash
npm run tauri build
```

Expected: an NSIS installer under `src-tauri/target/release/bundle/nsis/`. Record its size.

- [ ] **Step 4: Fold the deviations into the spec**

Update the spec:
- §4.2–4.4: six fixed slots and paging, the inner light and blur cross-fade approximations, the solid pill, reduced transparency from the registry.
- §4.5 item 2: Split semantics.
- §5: `tools_only`, `warn_over_s`/`warn_text`, `pdf-extract`.
- §6.1: thumbnails via `thumbnail()`, the allowlist and the engine rules.
- §6.5: schema 2, `options`.
- §6.8: luminance in the webview.
- §6.11: `start_jobs(preset_id, paths, options?)`, `thumbnail`, `capture_backdrop` now built, `pad_ready`/`pad_close`/`pad_track_cursor`, events `pad://open{paths, mode, reduced_transparency}` and `pad://cursor`.
- §6.13: the repo layout with `src/pad/` and the new Rust modules.
- Status line: Plan 2 implemented.

Add to §9's manual checklist:
- the pad excluded from its own capture
- light and dark wallpapers flipping segments
- `?perf` frame time on integrated graphics
- Tools reordering with mouse and keyboard

Update the README feature list.

- [ ] **Step 5: Handoff for Plan 3**

Write `docs/superpowers/handoff-plan-3.md` in the same shape as `handoff-plan-2.md`: where things stand, what to read, Plan 3's scope (the spec's "Deferred to Plan 3" list), the owner's working rules, the gotchas (unchanged plus anything learnt in Plan 2), open items (the manual checks still needing a human, plus every review finding left unfixed), and first steps. Plan 3's triggers call `pad::open(app, paths, PadMode::Drag | Click)`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "docs: fold Plan 2 decisions into the spec; README; handoff for Plan 3"
```

**→ Final Opus review of the whole branch, fix round, then report.**
