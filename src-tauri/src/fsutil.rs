use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Every file under `root`, recursively (no symlink following).
pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                out.push(e.path());
            }
        }
    }
    out
}

pub fn dir_size(root: &Path) -> u64 {
    walk_files(root).iter().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum()
}

/// fs::rename, retried with backoff (100 ms, doubling) while antivirus or sync
/// clients hold a transient lock, up to `budget` in total.
pub fn rename_with_retry(from: &Path, to: &Path, budget: std::time::Duration) -> io::Result<()> {
    let budget = budget.as_millis() as u64;
    let mut wait = 100u64;
    let mut waited = 0u64;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if crate::platform::is_transient_lock(&e) && waited < budget => {
                let d = wait.min(budget - waited);
                std::thread::sleep(std::time::Duration::from_millis(d));
                waited += d;
                wait *= 2;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_walks() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("sub")).unwrap();
        fs::write(d.path().join("a.txt"), "abc").unwrap();
        fs::write(d.path().join("sub").join("b.txt"), "de").unwrap();
        assert_eq!(
            sha256_file(&d.path().join("a.txt")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(walk_files(d.path()).len(), 2);
        assert_eq!(dir_size(d.path()), 5);
    }
}
