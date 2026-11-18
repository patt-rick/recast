//! Parse `magick -list format` into "magick:read:<fmt>" / "magick:write:<fmt>" (spec §6.3).
use std::collections::BTreeSet;

pub fn parse_magick_formats(out: &str) -> BTreeSet<String> {
    let mut caps = BTreeSet::new();
    let mut in_table = false;
    for line in out.lines() {
        if line.trim_start().starts_with("---") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 3 {
            continue;
        }
        // Some builds print a Module column between Format and Mode, some don't.
        let is_mode =
            |s: &str| s.len() == 3 && s.bytes().all(|c| matches!(c, b'r' | b'w' | b'+' | b'-'));
        let Some(mode) = t[1..3]
            .iter()
            .copied()
            .find(|s| is_mode(s))
            .map(|s| s.as_bytes())
        else {
            continue;
        };
        let fmt = t[0].trim_end_matches('*').to_ascii_lowercase();
        if mode[0] == b'r' {
            caps.insert(format!("magick:read:{fmt}"));
        }
        if mode[1] == b'w' {
            caps.insert(format!("magick:write:{fmt}"));
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "   Format  Module    Mode  Description
-------------------------------------------------------------------------------
      3FR  DNG       r--   Hasselblad CFV/H3D39II Raw Format (0.22.0-Release)
     AVIF  HEIC      rw+   AV1 Image File Format (1.17.6)
     HEIC  HEIC      r--   High Efficiency Image Format (1.17.6)
      JXL* JXL       rw-   JPEG XL (ISO/IEC 18181) (libjxl 0.10.3)
                           continuation of a description line
* native blob support
";

    #[test]
    fn parses_modes() {
        let c = parse_magick_formats(SAMPLE);
        assert!(c.contains("magick:read:avif") && c.contains("magick:write:avif"));
        assert!(c.contains("magick:read:heic") && !c.contains("magick:write:heic"));
        assert!(c.contains("magick:write:jxl"));
        assert!(c.contains("magick:read:3fr"));
        assert_eq!(c.len(), 6);
    }

    #[test]
    fn parses_builds_without_a_module_column() {
        let c = parse_magick_formats(
            "   Format  Mode  Description
-------------------------------------------------------------------------------
     AVIF  rw+   AV1 Image File Format (1.23.7)
     HEIC  r--   High Efficiency Image Format (1.23.7)
      JXL* rw+   JPEG XL (ISO/IEC 18181) (libjxl 0.13.0)
",
        );
        assert!(
            c.contains("magick:write:avif")
                && c.contains("magick:read:heic")
                && !c.contains("magick:write:heic")
        );
        assert!(c.contains("magick:write:jxl"));
        assert_eq!(c.len(), 5);
    }
}
