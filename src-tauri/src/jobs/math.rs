//! Shrink bitrate and disk-space estimates (spec §5.1 Shrink, §7 DISK_FULL).
use crate::presets::Preset;

pub const MB: u64 = 1024 * 1024;

/// Video kbps so a two-pass encode lands ≤ target (3% container margin).
/// Err(min_mb) when the result would fall below 150 kbps.
pub fn shrink_video_kbps(target_mb: u32, duration_s: f64, has_audio: bool) -> Result<u32, f64> {
    let target_bits = target_mb as f64 * MB as f64 * 8.0;
    let audio = if has_audio { 128.0 } else { 0.0 };
    let kbps = target_bits * 0.97 / duration_s / 1000.0 - audio;
    if kbps < 150.0 {
        Err((150.0 + audio) * 1000.0 * duration_s / 0.97 / 8.0 / MB as f64)
    } else {
        Ok(kbps.floor() as u32)
    }
}

/// Rough output size (spec §7): Shrink = target; WAV/AIFF ≈ 176 kB/s;
/// PDF → images ≈ 1 MB/page; otherwise ≈ the input size.
pub fn estimate_bytes(preset: &Preset, inputs_size: u64, duration_s: Option<f64>, pages: Option<u32>, target_mb: u32) -> u64 {
    if preset.uses("video_kbps") {
        return target_mb as u64 * MB;
    }
    if matches!(preset.out_ext.as_str(), "wav" | "aiff") {
        if let Some(d) = duration_s {
            return (d * 176_400.0) as u64;
        }
    }
    if preset.engine == "ghostscript" && preset.out_naming.is_folder() {
        return pages.map(|p| p as u64 * MB).unwrap_or(inputs_size.saturating_mul(10));
    }
    inputs_size
}

pub fn required_free(estimate: u64) -> u64 {
    ((estimate as f64 * 1.2) as u64).max(50 * MB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{Catalog, BUNDLED};

    #[test]
    fn shrink_bitrate() {
        assert_eq!(shrink_video_kbps(25, 100.0, true), Ok(1906));
        assert_eq!(shrink_video_kbps(25, 100.0, false), Ok(2034));
        let min = shrink_video_kbps(25, 3600.0, true).unwrap_err();
        assert!((min - 123.0).abs() < 1.0, "{min}");
    }

    #[test]
    fn estimates() {
        let c = Catalog::parse(BUNDLED).unwrap();
        assert_eq!(estimate_bytes(c.get("video-shrink").unwrap(), 900 * MB, Some(60.0), None, 25), 25 * MB);
        assert_eq!(estimate_bytes(c.get("video-wav").unwrap(), MB, Some(10.0), None, 25), 1_764_000);
        assert_eq!(estimate_bytes(c.get("pdf-images").unwrap(), MB, None, Some(30), 25), 30 * MB);
        assert_eq!(estimate_bytes(c.get("video-mp4").unwrap(), 7 * MB, None, None, 25), 7 * MB);
        assert_eq!(required_free(MB), 50 * MB);
        assert_eq!(required_f(100), 120);
    }

    fn required_f(mb: u64) -> u64 {
        required_free(mb * MB) / MB
    }
}
