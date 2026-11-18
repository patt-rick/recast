//! Rotating log at %LOCALAPPDATA%\Recast\logs (5 × 1 MB). Paths only, never file contents.
use std::path::Path;

use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};

pub fn init(dir: &Path) {
    let started = Logger::try_with_str("info").and_then(|l| {
        l.log_to_file(FileSpec::default().directory(dir).basename("recast").suppress_timestamp())
            .append()
            .rotate(Criterion::Size(1_000_000), Naming::Numbers, Cleanup::KeepLogFiles(4))
            .start()
    });
    match started {
        Ok(handle) => {
            Box::leak(Box::new(handle));
        }
        Err(e) => eprintln!("logging disabled: {e}"),
    }
}
