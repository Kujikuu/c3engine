//! Byte-shift obfuscation used by some TQ text tables (e.g. `ini/Play.dat`: every byte
//! is stored `+6`; the client reads `(c - 6) & 0xFF`, see `ReadPlayDatValue` in
//! Game/GameDlg.cpp). Decoded text is ordinary ini, parsed with [`crate::ini::Ini`].

/// Shift applied to `ini/Play.dat`.
pub const PLAY_DAT_SHIFT: u8 = 6;

/// Reverses the obfuscation: `stored - shift` per byte.
pub fn decode(stored: &[u8], shift: u8) -> Vec<u8> {
    stored.iter().map(|b| b.wrapping_sub(shift)).collect()
}

/// Applies the obfuscation: `plain + shift` per byte.
pub fn encode(plain: &[u8], shift: u8) -> Vec<u8> {
    plain.iter().map(|b| b.wrapping_add(shift)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_is_reversible() {
        let plain = b"[Start]\r\nX=1\r\n\xff\x00";
        assert_eq!(decode(&encode(plain, 6), 6), plain);
    }
}
