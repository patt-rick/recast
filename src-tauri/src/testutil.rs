//! Test-only helpers.
use std::path::PathBuf;

/// target/<profile>/examples/fake_engine.exe (built by plain `cargo test`).
pub fn fake_engine() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    let exe = p.join("examples").join("fake_engine.exe");
    assert!(
        exe.exists(),
        "{} is missing: run plain `cargo test` (it builds examples)",
        exe.display()
    );
    exe
}
