//! The backdrop under the pad (spec §6.8): captured in memory, never written to disk.
//! Wire format sent to the pad webview: u32 LE width, u32 LE height, then RGBA rows top-down.

pub const HEADER: usize = 8;
/// Larger captures are refused (a 368 px pad at 400% DPI is 1472 px).
pub const MAX_SIDE: u32 = 4096;

/// BGRA (GDI order, alpha undefined) → the wire format, alpha forced opaque.
pub fn encode(w: u32, h: u32, bgra: &[u8]) -> Result<Vec<u8>, String> {
    let n = (w as usize) * (h as usize) * 4;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("bad capture size {w}x{h}"));
    }
    if bgra.len() != n {
        return Err(format!("expected {n} bytes, got {}", bgra.len()));
    }
    let mut out = Vec::with_capacity(HEADER + n);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    for px in bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_to_rgba_and_forces_alpha() {
        let out = encode(2, 1, &[1, 2, 3, 0, 10, 20, 30, 7]).unwrap();
        assert_eq!(&out[..8], &[2, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&out[8..], &[3, 2, 1, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn rejects_wrong_sizes() {
        assert!(encode(0, 1, &[]).is_err());
        assert!(encode(2, 2, &[0; 4]).is_err());
        assert!(encode(MAX_SIDE + 1, 1, &vec![0; (MAX_SIDE as usize + 1) * 4]).is_err());
    }
}
