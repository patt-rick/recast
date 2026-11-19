# Recast

Drop a file, pick a conversion, get the result next to the original. Windows 10/11.
Recast wraps FFmpeg, ImageMagick, Pandoc, qpdf and Ghostscript, which download the first time they're needed.

Status: Plans 1 and 2 are implemented. The edge handle, desk pad, shake trigger, Explorer entry, first run and start with Windows come in Plan 3.

What works now:

- **Conversion core:** video, audio, image, PDF and text presets with real progress, cancelling, a stall watchdog and outputs that never overwrite anything. Engines download on demand and are checksum-pinned.
- **Drop pad:** a ring of six Liquid Glass segments with a thumbnail of your file in the middle. It adapts to light and dark backdrops, pages with More/Back and is fully keyboard-driven. For now it opens from the tray (**Convert files…** or **Drop pad**), and a drop on the Activity view opens the same ring inline.
- **Tools:** Merge PDFs and Images → PDF with a reorderable list (mouse or keyboard), and Split PDF by page ranges such as `1-3, 5, 8-`.
- **Companion window:** Activity, Tools, Library (engines and licences) and Settings.

Design: `docs/superpowers/specs/2026-11-18-recast-design.md`

## Develop

    npm install
    npm run tauri dev      # run the app
    npm test               # front-end tests
    npm run tauri build    # release build and NSIS installer
    cd src-tauri && cargo test                     # Rust tests (no engines needed)
    cd src-tauri && cargo test --features engines --test engines_integration -- --test-threads=1
                                                   # real-engine tests (downloads ~250 MB once)
    cd src-tauri && cargo clippy --all-targets -- -D warnings
    cd src-tauri && cargo fmt --check

To run a subset of the Rust tests, pass a filter after `--`, for example `cargo test -- jobs::`. The job and watchdog tests run against a fake engine (`src-tauri/examples/fake_engine.rs`), which `cargo test` builds as an example first; if you run a single test binary directly, run `cargo build --examples` first.

## Installer

`npm run tauri build` writes `src-tauri/target/release/bundle/nsis/Recast_0.1.0_x64-setup.exe`, a per-user installer of **5.9 MB** (6,179,116 bytes). The engines aren't in it. They download on first use, and the full 7-Zip 26.06 (`7z.exe` + `7z.dll`) ships inside the app to unpack them.

Recast's own code is MIT. Engines keep their own licences (Library → Third-party notices).
