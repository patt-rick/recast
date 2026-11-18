# Recast

Drop a file, pick a conversion, get the result next to the original. Windows 10/11.
Recast wraps FFmpeg, ImageMagick, Pandoc, qpdf and Ghostscript, which download the first time they're needed.

Status: Plan 1 (the core: engines, presets, jobs, history, settings and the companion window) is implemented. The drop pad, triggers and Explorer entry come in Plans 2 and 3.

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
