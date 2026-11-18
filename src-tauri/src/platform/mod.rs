//! The only module allowed to call OS APIs (spec §6.9).
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use self::win::*;

#[cfg(not(windows))]
compile_error!("Recast v1 targets Windows only; add a platform implementation (spec §6.9).");
