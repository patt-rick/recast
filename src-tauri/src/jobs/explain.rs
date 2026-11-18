//! Plain-language reasons from engine output (spec §7, ENGINE_FAILED).
const RULES: &[(&[&str], &str)] = &[
    (
        &["no space left on device", "not enough space on the disk"],
        "The disk is full.",
    ),
    (&["password", "encrypted"], "The PDF is password-protected."),
    (
        &[
            "invalid data found when processing input",
            "moov atom not found",
            "corrupt",
            "damaged",
            "error reading",
        ],
        "The file looks damaged or incomplete.",
    ),
    (
        &["unknown encoder", "encoder not found"],
        "This build of the converter can't write that format.",
    ),
    (
        &[
            "does not contain any stream",
            "matches no streams",
            "output file is empty",
        ],
        "The file has no audio or video the converter could use.",
    ),
    (
        &[
            "no decode delegate",
            "nodecodedelegateforthisimageformat",
            "no encode delegate",
        ],
        "The installed ImageMagick can't handle this image format.",
    ),
    (
        &[
            "unknown input format",
            "unknown reader",
            "unknown output format",
        ],
        "The converter doesn't recognise this document format.",
    ),
    (
        &["permission denied", "access is denied"],
        "Windows refused access to the file.",
    ),
];

pub const FALLBACK: &str = "The converter reported an error.";

pub fn explain(lines: &[String]) -> String {
    let lower: Vec<String> = lines.iter().map(|l| l.to_lowercase()).collect();
    for (needles, msg) in RULES {
        if needles.iter().any(|n| lower.iter().any(|l| l.contains(n))) {
            return msg.to_string();
        }
    }
    FALLBACK.to_string()
}

pub fn is_disk_full(lines: &[String]) -> bool {
    explain(lines) == RULES[0].1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn maps_known_patterns() {
        assert_eq!(
            explain(&v(&["x.mov: Invalid data found when processing input"])),
            "The file looks damaged or incomplete."
        );
        assert_eq!(
            explain(&v(&[
                "**** Error: This file requires a password for access."
            ])),
            "The PDF is password-protected."
        );
        assert_eq!(
            explain(&v(&[
                "magick: no decode delegate for this image format `HEIC'"
            ])),
            "The installed ImageMagick can't handle this image format."
        );
        assert!(is_disk_full(&v(&[
            "av_interleaved_write_frame(): No space left on device"
        ])));
        assert_eq!(explain(&v(&["something odd"])), FALLBACK);
    }
}
