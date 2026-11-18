//! Output names (spec §5.7): never overwrite, number on collision.
use std::path::{Path, PathBuf};

use super::OutNaming;

pub fn base_name(source: &Path, naming: &OutNaming) -> String {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    match naming {
        OutNaming::ReplaceExt => stem,
        OutNaming::Suffix(s) | OutNaming::Folder(s) => format!("{stem} ({s})"),
        OutNaming::Fixed(n) => n.clone(),
    }
}

/// `dir/base.ext`, else `dir/base (1).ext`, `dir/base (2).ext`, … (no ext for folders).
pub fn unique_path(
    dir: &Path,
    base: &str,
    ext: Option<&str>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let name = |n: u32| {
        let b = if n == 0 {
            base.to_string()
        } else {
            format!("{base} ({n})")
        };
        match ext {
            Some(e) => format!("{b}.{e}"),
            None => b,
        }
    };
    let mut n = 0;
    loop {
        let p = dir.join(name(n));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

pub fn final_path(
    dir: &Path,
    source: &Path,
    naming: &OutNaming,
    ext: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let ext = if naming.is_folder() { None } else { Some(ext) };
    unique_path(dir, &base_name(source, naming), ext, exists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    fn exists_in(set: &[&str]) -> impl Fn(&Path) -> bool {
        let s: HashSet<PathBuf> = set.iter().map(|n| Path::new("C:/d").join(n)).collect();
        move |p: &Path| s.contains(p)
    }

    #[test]
    fn replace_ext_and_collisions() {
        let src = Path::new("C:/d/holiday.mov");
        let none = exists_in(&[]);
        assert_eq!(
            final_path(Path::new("C:/d"), src, &OutNaming::ReplaceExt, "mp3", &none),
            Path::new("C:/d/holiday.mp3")
        );
        let taken = exists_in(&["holiday.mp3", "holiday (1).mp3"]);
        assert_eq!(
            final_path(
                Path::new("C:/d"),
                src,
                &OutNaming::ReplaceExt,
                "mp3",
                &taken
            ),
            Path::new("C:/d/holiday (2).mp3")
        );
    }

    #[test]
    fn suffix_folder_and_fixed() {
        let src = Path::new("C:/d/holiday.mp4");
        let none = exists_in(&[]);
        assert_eq!(
            final_path(
                Path::new("C:/d"),
                src,
                &OutNaming::Suffix("compressed".into()),
                "mp4",
                &none
            ),
            Path::new("C:/d/holiday (compressed).mp4")
        );
        assert_eq!(
            final_path(
                Path::new("C:/d"),
                Path::new("C:/d/report.pdf"),
                &OutNaming::Folder("pages".into()),
                "png",
                &none
            ),
            Path::new("C:/d/report (pages)")
        );
        let taken = exists_in(&["merged.pdf"]);
        assert_eq!(
            final_path(
                Path::new("C:/d"),
                Path::new("C:/d/a.pdf"),
                &OutNaming::Fixed("merged".into()),
                "pdf",
                &taken
            ),
            Path::new("C:/d/merged (1).pdf")
        );
    }
}
