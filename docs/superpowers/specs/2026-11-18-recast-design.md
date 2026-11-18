# Recast — Design Spec

- **Date:** 2026-11-18
- **Status:** Draft, awaiting owner review
- **Platform (v1):** Windows 10 (2004+) and Windows 11, x64
- **Visual reference:** `docs/mockups/pad-liquid-glass.html`, variant 4 ("Regular, adapting"). It was built inside the brainstorm companion frame, so open it there for full styling. The geometry and effect values are copied into §4.4.

---

## 1. What Recast is

Recast is a Windows file-conversion utility built around one gesture. You drag a file onto a ring of glass segments, each segment is a conversion that file supports, and you release on one. The converted file appears next to the original. There is no dialog and no save prompt.

It wraps existing open-source engines (FFmpeg, ImageMagick, Pandoc, qpdf, Ghostscript) and implements no codecs or format logic of its own. Its value is the experience: the drop pad, reliability (real progress, cancelling, stall detection) and engines that download on demand.

### 1.1 Why it exists (prior art)

The closest existing tool is **File Converter** (Tichau/FileConverter): 15.7k stars, GPL-3.0, v2.2 released 2026-02-13. Its most-requested gaps are the ones Recast is designed around:

| File Converter gap (issue #) | Recast answer |
|---|---|
| No main window; asks for drag-and-drop (#518) | Drop pad plus a companion window that also accepts drops |
| No dark mode (#240, #771) | Liquid Glass that adapts light/dark to what's behind it |
| Missing from Win 11 menu / menu entry disappears (#211, #819, #465, #604) | The pad is the main way in. One classic menu entry, "Open with Recast" |
| ffmpeg freezes and stuck conversions (#749, #780, #700, #739) | Stall watchdog, process-tree kill, Job Objects |
| AVIF, JPEG XL, Opus, M4A, ALAC, AIFF (#226, #427, #111, #245, #437, #220) | All in the v1 preset catalog |
| Merge PDFs, convert whole folders (#26, #94) | PDF merge in v1. Folders are a non-goal for v1 (§11) |
| Needs Office for Word→PDF (#778, #717) | Documents are deferred and will come via a LibreOffice download (§11) |

Recast is **not a fork**. It shares no code with File Converter and is not bound by its GPL.

---

## 2. Decisions (locked during brainstorming)

| # | Decision | Choice |
|---|---|---|
| D1 | Engine strategy | Thin wrappers over existing CLI engines (not driving File Converter, not a fork) |
| D2 | v1 categories | Video, Audio, Images, PDF tools, Ebooks/text. **Documents (Office→PDF) deferred** because LibreOffice is about 350 MB |
| D3 | Stack | Tauri 2: Rust core, React + TypeScript + Vite front end |
| D4 | Engine delivery | Small installer. Each engine downloads the first time it's needed and is cached for offline use |
| D5 | Primary UX | Drop pad: a ring of tools around the dropped file |
| D6 | Pad visual | Segmented ring in Apple Liquid Glass language, "regular" variant, adapting light/dark per segment |
| D7 | Pad triggers | Edge handle, desktop pad, and shake-while-dragging. All three can be toggled in Settings |
| D8 | Companion window | Small tray window: Activity (also a drop target), Tools, Library (engines), Settings |
| D9 | Explorer integration | One classic-menu entry, "Open with Recast", that opens the pad with the selection loaded |
| D10 | macOS | Later. v1 isolates OS-specific code behind a `platform` module (§6.9) |
| D11 | Name / location | **Recast**, repo at `Desktop/repos/recast` |

---

## 3. Goals and non-goals

**Goals (v1)**
1. Any supported file can be converted in **one drag**, with no window to manage.
2. Every job shows **real progress where the engine reports it**, can be **cancelled**, and **never hangs forever**.
3. **Outputs never overwrite** anything, and a failed job leaves no partial files.
4. The installer is about 15 MB; engines arrive only when first needed.
5. The pad feels native-quality: it runs at 60 fps on a mid-range laptop and adapts to light and dark backdrops.

**Non-goals (v1)**: Office documents, EPUB→PDF, folder recursion, trimming or editing media, GPU encoding, the Windows 11 modern context menu, code signing, macOS/Linux, auto-update of the app itself, telemetry.

---

## 4. User experience

### 4.1 Ways to open the pad

| Trigger | Default | Behaviour |
|---|---|---|
| **Edge handle** | On | A 6 × 80 px translucent tab docked to a screen edge (right, at 50% height by default; edge and position are configurable; primary monitor). A drag entering it opens the pad anchored next to the handle. |
| **Desktop pad** | Off | A 96 px round glass widget sitting at desktop level (not always-on-top), draggable, position remembered. A drag entering it opens the pad over it. |
| **Shake while dragging** | On | While the left button is held and the pointer is moving, **≥3 horizontal direction reversals within 500 ms, each ≥40 px**, open the pad centred at the cursor. If no drop arrives within 1.5 s of the button being released, the pad closes on its own. |
| **"Open with Recast"** (Explorer) | On | Opens the pad at the cursor with the selection preloaded. Segments are then **clicked** rather than dropped on. |
| **Companion → Activity** | Always | Dropping files on the Activity view opens the same ring inside the window. |

### 4.2 Pad anatomy

- **Centre disc:** the file's thumbnail (first video frame, image preview, PDF page 1, or a type glyph), plus its name and meta, e.g. `MOV · 1.2 GB · 2:14`. With several files: a stacked thumbnail and "8 files".
- **Ring:** **six segments** in a fixed clockwise order starting at 12 o'clock. Each shows a line icon, a short label (`MP3`) and, on hover, a pill outside the ring with the full description (`Audio only · MP3`).
- **More:** if more than six tools apply, segment 6 becomes **More**. Activating it (hovering for 600 ms mid-drag, or clicking) cross-fades the ring to the next page of tools, where segment 1 becomes **Back**.
- **Engine badge:** a segment whose engine isn't installed shows a small download badge (§7.1).

### 4.3 Interaction states

```
closed ──trigger──► opening (materialize, 520 ms)
opening ──► idle ──pointer near segment──► targeting(segment i)
targeting ──drop / click / Enter──► running
idle/targeting ──Esc / drop outside the segments / drag leaves pad >80 px for 600 ms──► closed
running ──► done (✓ for 1.2 s) ──► closed
running ──► failed (⚠ for 2 s, notification) ──► closed
```

- **Running:** the segments retract into the disc and the disc's rim becomes a progress ring. It's indeterminate when the engine reports no progress (§6.4). The pad can be dismissed with Esc or by clicking outside it. The job keeps running and appears in Activity.
- **Done:** Windows notification, "holiday.mp3 ready", with a **Show in folder** action.
- **Failed:** notification, "Couldn't convert holiday.mov", with a **Details** action that opens Activity at that job.
- **Keyboard:** when the pad is open, ←/→ (or Tab) move around the ring, Enter runs the focused segment, and Esc closes.

### 4.4 Visual spec (Liquid Glass, regular, adapting)

Values are at 1× scale and scale with DPI. The source of truth is the mockup's `regular()` and `rsector()`.

| Property | Value |
|---|---|
| Pad window | 440 × 440 logical px, centred on the anchor, clamped to the monitor's work area |
| Ring | centre radius R = 136, thickness 68, 6 segments, **parallel gaps** of 9 px, corner radius 17. All shapes concentric with the disc |
| Centre disc | radius R − 34 − 14 = 88 |
| Glass fill | the backdrop snapshot (§6.8) clipped to the segment, **magnified 1.10×** about the segment centre (1.15× when targeted), blurred 5 px (6 px over light), saturate 1.6, brightness 1.08 |
| Rim | 1.2 px stroke, diagonal gradient: white 95% (top-left) → 25% → 12% → 60% (bottom-right) |
| Inner light | 7 px white stroke at 35% opacity (52% targeted), clipped inside the shape, blurred 2.2 px |
| Shadow | `0 8px 16px rgba(0,0,0,.16)`, `0 1px 2px rgba(0,0,0,.12)` |
| Targeted segment | lifts 9 px outward, +8% thickness, +4% angular width, radial glow (r = 80) at the pointer, tint system blue `rgba(10,132,255,.45)` (`.80` over light). All values spring-smoothed (lerp 0.2 per frame) |
| Light adaptation | per segment: if the mean luminance of the backdrop under it is > 0.6, use light glass (tint white 12%, brightness 1.0, saturate 1.2) and **dark symbols** `#1d1d1f`; a targeted segment returns to white symbols on blue |
| Open animation | segments materialize with a 55 ms stagger and 520 ms ease-out-cubic: angular width 55%→100%, thickness 80%→100%, magnification 1.0→1.10, blur 0→5 px |
| Type | Segoe UI Variable (system), labels 11 px semibold; icons 23 px line icons, 1.75 stroke |
| Reduced transparency (Windows setting) | glass replaced by a solid fill (`#2b2b30` 92% / `#f2f2f5` 92%) with rims kept |
| Reduced motion | no lift, scale or materialize animation. Opening is instant and targeting is a tint change only |

### 4.5 Companion window

A 760 × 520 window opened from the tray icon (left-click) or the tray menu. Same glass language, with a sidebar of four sections:

1. **Activity:** running jobs (progress, ETA, Cancel) above history (newest first: Show in folder, Open, Retry, Remove from list). Dropping files anywhere in the view opens the ring inline. Failed jobs expand to show a plain-language reason and the last 30 lines of engine output.
2. **Tools:** multi-file jobs that need ordering. **Merge PDFs** and **Images → PDF** use a reorderable list (drag handles, or add files) and a Run button. **Split PDF** takes a page-range field (`1-3, 5, 8-`).
3. **Library:** one row per engine: name, version, size, licence and status (Not installed / Installing x% / Installed / Damaged). Actions: Download, Remove, Verify. A footer shows the total disk used. Future engines (LibreOffice, LaTeX) appear here as "Coming later".
4. **Settings:**
   - **Triggers:** on/off for each; edge and position for the edge handle.
   - **Defaults:** quality per format (§5).
   - **Output:** next to the original, or a fixed folder.
   - **Concurrency:** 1–4, default 2.
   - **Stall timeout:** default 60 s.
   - **Start with Windows:** default on.
   - **Explorer entry:** on/off.

Closing the window hides it. Recast keeps running in the tray, and "Quit" in the tray menu exits.

---

## 5. Preset catalog (v1)

Presets are **data**, in `src-tauri/presets/presets.toml`. Each preset has:
- `id`, `label`, `description`, `icon`
- `accepts` (kinds and extensions), `min_files`, `engine`
- `args` (a template with `{in}`, `{out}` and `{setting.*}` placeholders)
- `out_ext`, `out_naming` (`replace_ext` | `suffix:<text>` | `folder:<text>`)
- `progress` (`ffmpeg` | `ghostscript_pages` | `none`)

The ring order is the order in the table, and anything past five spills into More.

### 5.1 Video (`mp4 mkv mov webm avi wmv flv m4v mpg ts 3gp`)
| Preset | Engine | Core args / notes |
|---|---|---|
| MP3 | ffmpeg | `-vn -c:a libmp3lame -b:a {mp3_kbps}k` (default 192) |
| MP4 | ffmpeg | `-c:v libx264 -crf 23 -preset medium -c:a aac -b:a 160k -movflags +faststart` |
| GIF | ffmpeg | `fps={gif_fps},scale={gif_width}:-1:flags=lanczos` with palettegen/paletteuse (defaults 12 fps, 480 px). The job warns if the source is > 60 s |
| WebM | ffmpeg | `-c:v libvpx-vp9 -crf 32 -b:v 0 -c:a libopus -b:a 128k` |
| Shrink | ffmpeg | two-pass x264 to **{target_mb}** MB (default 25): video kbps = target_bits / duration − audio 128k. **Fails up front with a reason** if the result would be < 150 kbps. Output `name (compressed).mp4` |
| M4A | ffmpeg | `-vn -c:a aac -b:a {aac_kbps}k` (default 192) |
| More → MKV, MOV, Opus, WAV | ffmpeg | remux where the codecs allow (`-c copy`), otherwise re-encode |

### 5.2 Audio (`mp3 m4a aac wav flac ogg opus wma aiff aif alac`)
MP3, M4A, Opus (`libopus 128k`), FLAC, WAV (`pcm_s16le`) · More → ALAC (`.m4a`, `-c:a alac`), AIFF, OGG Vorbis. A preset whose output matches the input format is hidden, except Shrink.

### 5.3 Images (`jpg jpeg png webp avif heic heif jxl bmp tif tiff gif ico`)
| Preset | Engine | Notes |
|---|---|---|
| JPG | magick | quality {jpg_q} (85). Alpha is flattened on white |
| PNG | magick | lossless, alpha preserved |
| WebP | magick | quality {webp_q} (82), alpha preserved |
| AVIF | magick | quality {avif_q} (60) |
| Shrink | magick | longest edge ≤ {shrink_px} (2048) **and** quality 80, same format. `name (compressed).ext` |
| More → JPEG XL, ICO (16/32/48/256, alpha preserved), TIFF, PDF | magick | ICO keeps transparency (File Converter #63) |
| **Combine into PDF** (≥2 images) | magick | appears instead of More's PDF entry. Uses drop order; the Tools page allows reordering |

### 5.4 PDF (`pdf`)
| Preset | Engine | Notes |
|---|---|---|
| Compress | gs | `-sDEVICE=pdfwrite -dPDFSETTINGS=/ebook`. `name (compressed).pdf` |
| To images | gs | `png16m`, 150 dpi, into folder `name (pages)/page-001.png` |
| Split | qpdf | `--split-pages` into folder `name (pages)/` |
| **Merge** (≥2 PDFs) | qpdf | `--empty --pages a.pdf b.pdf … --`, in drop order. Output `merged.pdf` in the first file's folder |

### 5.5 Ebooks / text (`epub docx odt md markdown html htm txt rtf`)
EPUB, DOCX, HTML (`--standalone`), Markdown (`gfm`), ODT, all via pandoc. PDF output is excluded (it needs a LaTeX engine, §11).

### 5.6 Multiple files
- **Same kind:** the ring shows that kind's presets, and each file becomes its own job. The multi-file presets (Merge, Combine into PDF) appear when `min_files` is met.
- **Mixed kinds:** the ring shows only presets whose `accepts` covers **every** file. If none do, the ring has a single segment, **Open in Recast**, which opens Activity with the files listed so each group can be converted separately.

### 5.7 Output naming and location
- **Default location:** the source file's folder. Settings can switch to a fixed folder.
- **Name:** `replace_ext` gives `holiday.mp3`. If that exists, `holiday (1).mp3`, `holiday (2).mp3`, and so on. **Nothing is ever overwritten**, and the source is never modified.
- Same-format presets use a suffix (`holiday (compressed).mp4`). Presets that output folders use `name (pages)/`.

---

## 6. Architecture

```
┌────────────── Tauri app (single instance, tray-resident) ──────────────┐
│  Webviews (React+TS)                                                    │
│   ├─ pad        (transparent, frameless, topmost, excluded from capture)│
│   ├─ handle     (edge tab)        ├─ desk-pad (desktop widget)          │
│   └─ companion  (Activity · Tools · Library · Settings)                 │
│              ▲ invoke / events (typed IPC, §6.10)                       │
│  Rust core ─────────────────────────────────────────────────────────── │
│   detect · presets · engines · jobs · history · settings                │
│   triggers · shell · backdrop        └── platform (windows impl)        │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │ spawn (argv arrays, no shell)
              ffmpeg/ffprobe · magick · pandoc · qpdf · gswin64c
              (in %LOCALAPPDATA%\Recast\engines\<id>\<version>\)
```

**Data flow for one drop:** trigger → pad opens → `inspect(paths)` returns a `PadModel` (kinds, thumbnail, available presets with engine status) → the user targets a segment → `start_jobs(preset_id, paths)` → `jobs` checks preconditions (§7), ensures the engine, then spawns → `job://progress` events reach the pad and Activity → the temp file is renamed to its final name → `job://done` → notification.

### 6.1 `detect`
- **Input:** a path. **Output:** `FileInfo { path, kind: Video|Audio|Image|Pdf|Text|Unknown, ext, size, duration_s?, thumb? }`.
- Uses magic bytes (the `infer` crate) and falls back to the extension. Duration and thumbnail come from ffprobe/ffmpeg when FFmpeg is installed; otherwise the disc shows a type glyph.
- A `.mov` containing only audio counts as Audio (ffprobe stream check) when ffprobe is available.

### 6.2 `presets`
- Loads and validates `presets.toml` at startup. Validation checks for unique ids, known engines and placeholders, and a valid `out_naming`.
- `available(files) -> Vec<PresetView>` applies the §5.6 rules and ring order, and hides presets whose output format matches the input.
- `build_command(preset, files, settings, out_path) -> Command { program, args: Vec<OsString> }`. This is pure and has no I/O. Paths are always passed **absolute**, so a filename starting with `-` can't be mistaken for an option.

### 6.3 `engines`
- **The manifest** is bundled in the app: `engines.toml`, one entry per engine per platform. Each entry has:
  - `id`, `name`, `version`, `platform` (`windows-x64`)
  - `url` (an HTTPS, pinned release asset), `sha256`, `size`
  - `archive` (`zip` | `nsis-silent`), `bins` (logical name → relative exe path), `licence`, `homepage`
- **Install:** download to `engines\.partial\<id>` with resume, verify SHA-256, then extract into `engines\<id>\<version>\`. Ghostscript's NSIS installer runs as `/S /D=<dir>`, per user, with no admin rights. The folder is renamed into place atomically. A failed install leaves nothing behind.
- **Status:** `NotInstalled | Installing{pct} | Installed{version} | Damaged`. "Damaged" means a bin is missing or `Verify` found a hash mismatch for the extracted exe; a re-download fixes it.
- **Updating engines** happens through app releases that ship a newer manifest: Library shows "Update available" when the installed version ≠ the manifest version. A remote, unsigned manifest is deliberately **not** used, because it would be a code-execution channel.
- **Engine sources** (pinned in the manifest by a CI script, `scripts/refresh-engines`):
  - FFmpeg: BtbN win64 GPL zip, which includes ffprobe
  - ImageMagick: portable Q16-HDRI x64 zip
  - Pandoc: windows-x86_64 zip
  - qpdf: msvc64 zip
  - Ghostscript: gs w64 installer

### 6.4 `jobs`
- **Queue:** FIFO. Concurrency comes from settings (default 2). Multi-file presets count as one job.
- **Process control:** each job's process runs inside a Windows **Job Object** with `KILL_ON_JOB_CLOSE`. Cancelling, the watchdog, or Recast crashing therefore kills the **entire process tree**. stdout and stderr are both drained continuously, which prevents pipe-full deadlocks; the last 30 stderr lines are kept for Details.
- **Progress:**
  - `ffmpeg`: `-progress pipe:1 -nostats`, giving `out_time_us / duration`. Two-pass jobs report 0–50% for pass 1 and 50–100% for pass 2.
  - `ghostscript_pages`: the "Page N" lines against the page count (from qpdf `--show-npages`).
  - `none`: indeterminate. Batches report n/total files.
- **Watchdog:** for engines that report progress, no progress line for **{stall_timeout} s** (60) marks the job **Stalled** and kills it. For `none` engines, there's a hard cap of 10 min per file, configurable.
- **Atomic output:** the engine writes to `<dest>\.recast-<job>.<ext>` (hidden, same volume). On success it's renamed to the final name, with the collision check repeated at rename time. On failure or cancel, the temp file is deleted. Temp paths are recorded in `history` before spawning, so leftovers are deleted on the next launch.
- **Events:** `job://queued|progress|done|failed|cancelled`, all carrying `{ job_id, preset_id, file, pct?, eta_s?, out_path?, error? }`.

### 6.5 `history`
- SQLite at `%LOCALAPPDATA%\Recast\recast.db`, via `rusqlite`.
- **Tables:**
  - `jobs`: id, preset_id, inputs JSON, output, status, error_code, error_detail, stderr_tail, started_at, ended_at, temp_path
  - `meta`: schema version
- Keeps the last 500 jobs, and the oldest are pruned. "Remove from list" deletes only the row, never files.

### 6.6 `settings`
JSON at `%APPDATA%\Recast\settings.json`, with a typed struct, defaults for every field and a `version` for migrations. Unknown fields are kept; invalid values fall back to their defaults and are logged.

### 6.7 `triggers`
- **Edge handle** and **desk pad** are tiny Tauri windows. They use Tauri's drag-drop events (`Enter` / `Over{position}` / `Drop{paths}` / `Leave`), and Enter opens the pad.
- **Shake:** a `WH_MOUSE_LL` hook on a dedicated thread, implementing the detector in §4.1. The detector is a pure function over a sample stream, so it can be unit-tested. The hook does nothing while the left button is up.
- **Mid-drag targeting:** while a drag is over the pad window, `Over{position}` events drive hit-testing, because ordinary mouse events don't fire during an OLE drag. `Drop` fires the targeted segment.

### 6.8 `backdrop`
- On opening, the pad captures the screen rectangle under it via the platform layer (Windows: Desktop Duplication/GDI `BitBlt` of the monitor region).
- The pad window has `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, so it never captures itself and can re-capture if it moves.
- The image goes to the webview as an in-memory blob; it is never written to disk and is dropped when the pad closes. Per-segment luminance (for §4.4 light adaptation) is computed from the same image.
- If capture fails, the pad falls back to the reduced-transparency look.

### 6.9 `platform` (the macOS seam)
The following trait-like interfaces have a Windows implementation in v1; a macOS implementation is added later:

| Interface | Windows v1 | macOS later |
|---|---|---|
| `capture_region(rect)` | BitBlt / DXGI | native NSGlassEffect / vibrancy (no capture needed) |
| `global_drag_shake` | `WH_MOUSE_LL` | `CGEventTap` (Accessibility permission) |
| `shell_entry` install/remove | HKCU registry | Finder Quick Action |
| `kill_tree` / process group | Job Objects | process groups + `killpg` |
| `engine platform key` | `windows-x64` | `macos-arm64`, `macos-x64` |

Nothing outside `platform` may call OS APIs directly.

### 6.10 `shell` and single instance
- "Open with Recast" is registered under `HKCU\Software\Classes\*\shell\Recast` (no admin rights), with command `"<exe>" --pad "%1"`.
- Explorer starts one process per selected file. `tauri-plugin-single-instance` forwards them to the running instance, which **debounces for 300 ms** and opens one pad with all the files.
- The entry is removed when the Explorer-entry setting is turned off, and on uninstall.

### 6.11 IPC (typed, in `src/shared/ipc.ts` mirrored in Rust)
- **Commands:**
  - `inspect(paths)`
  - `start_jobs(preset_id, paths)`, `cancel_job(id)`, `retry_job(id)`, `list_jobs(page)`, `remove_job(id)`
  - `engines_list()`, `engine_install(id)`, `engine_remove(id)`, `engine_verify(id)`
  - `get_settings()`, `set_settings(patch)`
  - `capture_backdrop()`
  - `reveal(path)`, `open(path)`
- **Events:** `job://*`, `engine://progress|done|failed`, `pad://open{paths, anchor}`.

### 6.12 Repo layout
```
recast/
  src/                 # React + TS
    pad/               # PadApp, geometry.ts (rsector), hitTest.ts, glass/ (renderer), states.ts
    companion/         # Activity, Tools, Library, Settings
    handle/ deskpad/   # trigger windows
    shared/            # ipc.ts, types.ts, icons/
  src-tauri/
    src/  detect.rs presets.rs engines.rs jobs.rs history.rs settings.rs
          triggers.rs shell.rs backdrop.rs ipc.rs  platform/{mod.rs,windows.rs}
    presets/presets.toml
    engines/engines.toml
  scripts/refresh-engines   # re-pins engine URLs + sha256
  docs/
```

---

## 7. Error handling

| Situation | Behaviour |
|---|---|
| **Engine not installed** | The segment shows a download badge. Dropping on it queues the job as *Waiting for engine*, installs the engine (pad ring: "Getting FFmpeg · 84 MB"), then runs the job. If the install fails, the job fails with `ENGINE_INSTALL_FAILED`. |
| **Download or checksum failure** | The partial download is deleted and nothing is executed. Notification with **Retry**; Library shows the error. |
| **Engine exits non-zero** | `ENGINE_FAILED`. A plain-language reason is mapped from known stderr patterns (unsupported codec, corrupt input, encrypted PDF, …) and falls back to "The converter reported an error". The stderr tail is in Details. |
| **Stalled** | `STALLED`: killed by the watchdog and the temp file deleted. Details suggest Retry. |
| **Source missing, locked or unreadable** | `SOURCE_UNREADABLE`, checked before spawning. |
| **Destination not writable** | `DEST_UNWRITABLE`, checked before spawning by creating and deleting a probe file. |
| **Low disk space** | `DISK_FULL` before spawning, if free space < max(estimate × 1.2, 50 MB). Estimates: same-kind conversion ≈ source size; WAV ≈ duration × 176 kB/s; Shrink = target; PDF to images ≈ pages × 1 MB. An out-of-space failure during the run maps to the same code. |
| **Shrink target impossible** | `TARGET_TOO_SMALL`, before spawning, with the minimum achievable size in the message. |
| **Cancelled** | `CANCELLED`: process tree killed, temp file deleted, source untouched. |
| **App crash or quit mid-job** | The Job Object kills the children. On next launch, temp files recorded in `history` are deleted and those jobs are marked `INTERRUPTED` (Retry available). |
| **Backdrop capture fails** | The pad uses the reduced-transparency look; the error is logged, not shown. |
| **Shake false positive** | The pad auto-closes 1.5 s after mouse-up with no drop. No error. |

Logs go to `%LOCALAPPDATA%\Recast\logs\recast.log` (rotating, 5 × 1 MB). Logs and errors never include file *contents*, only paths.

---

## 8. Security and privacy
- **No network** except engine downloads from the pinned HTTPS URLs in the bundled manifest. No telemetry and no update pings in v1.
- Engines are run by absolute path from Recast's own folder, with argv arrays. **There is never a shell**, so filenames can't inject commands.
- Downloaded binaries run only after SHA-256 verification.
- The backdrop snapshot stays in memory and is discarded when the pad closes.
- Everything is installed per user, with no admin rights needed. The installer is unsigned in v1; SmartScreen will warn (§11).

---

## 9. Testing

| Layer | What | How |
|---|---|---|
| Rust unit | `presets::build_command` for every preset (snapshot tests of argv); `available()` rules for single, multiple and mixed files; output naming and collisions; ffmpeg progress parser; Ghostscript page parser; shake detector over recorded sample streams; settings defaults and migration; manifest validation | `cargo test`, no engines needed |
| Rust watchdog / kill | a fake engine (test binary) that prints progress then hangs, ignores signals, or spawns a child; assert Stalled, the whole tree killed and the temp file gone | `cargo test` |
| Rust integration | tiny fixtures (2 s video, short audio, PNG with alpha, HEIC, 3-page PDF, EPUB, MD) through **every preset** with the real engines (downloaded once, cached in CI). Assert the output exists, has the right type (`ffprobe` / `magick identify` / `qpdf --check`), alpha is kept where expected, and Shrink is ≤ the target | `cargo test --features engines` |
| Engines | install from the manifest, a corrupted download is rejected, an interrupted install leaves nothing, Verify detects tampering | `cargo test --features engines` |
| Front end | `rsector` geometry; hit-testing (point → segment, including gaps, More and Back); ring paging; light-adaptation threshold; state machine transitions (§4.3) | Vitest |
| Manual checklist | real Explorer drags onto the edge handle, desk pad and shake; "Open with Recast" with 1 and 20 files; 100%/150%/200% DPI; two monitors; reduced motion and reduced transparency; keyboard-only pad; dark and light wallpapers | `docs/qa-checklist.md`, run before each release |

---

## 10. Settings defaults (v1)

| Key | Default |
|---|---|
| triggers.edge_handle | on (right edge, 50%) |
| triggers.desk_pad | off |
| triggers.shake | on |
| shell.explorer_entry | on |
| output.location | next to original |
| quality.mp3_kbps / aac_kbps | 192 / 192 |
| quality.jpg_q / webp_q / avif_q | 85 / 82 / 60 |
| gif.fps / gif.width | 12 / 480 |
| shrink.target_mb / shrink.image_px | 25 / 2048 |
| jobs.concurrency | 2 |
| jobs.stall_timeout_s / jobs.no_progress_cap_min | 60 / 10 |
| app.start_with_windows | on |

---

## 11. Later (explicitly out of v1)
- **Documents** (Word/PowerPoint/Excel → PDF) via a LibreOffice download in Library.
- **EPUB/Markdown → PDF** via a LaTeX engine (Tectonic) download.
- **macOS port** through the §6.9 seam (requires an Apple Developer account and notarization).
- **Windows 11 modern context menu** (sparse MSIX + `IExplorerCommand`; requires a code-signing certificate).
- **Code signing** to remove the SmartScreen warning.
- **GPU encoding** (NVENC/QSV/AMF presets), folder recursion, trimming.
- **Two-tier quality ring** (hover to reveal 128/192/320 kbps), explored in brainstorming and set aside.

---

## 12. Open questions (owner)
1. **Recast's own licence:** MIT, GPL, or closed? Engines are only invoked and downloaded from upstream, so any choice works (§6.3).
2. **App icon / brand mark:** needed before the first installer build.
