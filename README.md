# Recast

Drop a file, pick a conversion, get the result next to the original. Windows 10/11.
Recast wraps FFmpeg, ImageMagick, Pandoc, qpdf and Ghostscript, which download the first time they're needed.

Status: Plans 1 to 3 are implemented. What remains is the release and the manual QA in `docs/qa-checklist.md`.

What works now:

- **Conversion core:** video, audio, image, PDF and text presets with real progress, cancelling, a stall watchdog and outputs that never overwrite anything. Engines download on demand and are checksum-pinned.
- **Drop pad:** a ring of six Liquid Glass segments with a thumbnail of your file in the middle. It adapts to light and dark backdrops, pages with More/Back and is fully keyboard-driven. Files of different kinds get one **Open** segment that lists them by kind in Activity.
- **How the pad opens:**
  - drag a file onto the **edge handle** (a thin tab on a screen edge, left by default), or onto the optional **desk pad**
  - **shake** a file left and right while you drag it
  - right-click files in Explorer, **Show more options**, **Open with Recast**, or run `recast.exe --pad <files>`
  - the tray menu's **Convert files…**
  - a drop on the Activity view opens the same ring inline
- **Tools:** Merge PDFs and Images → PDF with a reorderable list (mouse or keyboard), and Split PDF by page ranges such as `1-3, 5, 8-`.
- **Companion window:** Activity, Tools, Library (engines and licences) and Settings (triggers, output, quality, jobs, Explorer entry and Start with Windows). A first-run welcome explains the handle, the tray and the toggles.
- **Notifications** have buttons: Show in folder, Details, Retry, Save to fallback folder. They need an installed build (a dev build has no app identity for toasts).

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

A dev build (`npm run tauri dev`, `cargo run`) does **not** write the Explorer verb or the Start with Windows value, so it never points Explorer at `target\debug`. To test them from a dev build, set `RECAST_DEV_INTEGRATION=1` first (exactly `1`). Remember to turn the settings off, or uninstall, when you are done, because the entries then point at the dev exe. To open the pad from a terminal in any build, run `recast.exe --pad <files>`; with Recast already running, it forwards the files to that instance.

To run a subset of the Rust tests, pass a filter after `--`, for example `cargo test -- jobs::`. The job and watchdog tests run against a fake engine (`src-tauri/examples/fake_engine.rs`), which `cargo test` builds as an example first; if you run a single test binary directly, run `cargo build --examples` first.

## Installer

`npm run tauri build` writes `src-tauri/target/release/bundle/nsis/Recast_0.1.0_x64-setup.exe`, a per-user installer of about **6.2 MB**. The engines aren't in it. They download on first use, and the full 7-Zip 26.06 (`7z.exe` + `7z.dll`) ships inside the app to unpack them.

Recast's own code is MIT. Engines keep their own licences (Library → Third-party notices).

## CI and releases

- **CI** (`.github/workflows/ci.yml`) runs on pushes to `master` and on pull requests: front-end tests and build, then `cargo fmt --check`, clippy and the Rust tests (not the real-engine tests).
- **Release** (`.github/workflows/release.yml`): bump the version in `src-tauri/tauri.conf.json`, `package.json` and `src-tauri/Cargo.toml`, commit, then push a tag `vX.Y.Z`. The workflow builds the installer and publishes it with its SHA-256 on the GitHub release for that tag. It won't replace an installer a release already has. Run it by hand (Actions → Release → Run workflow) to get a build from the run's artifacts without touching any release.
- The installer isn't code-signed yet; signing goes between the build and the checksum steps.
