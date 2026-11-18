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
