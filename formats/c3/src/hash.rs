//! TQ legacy string id hash, used for WDF entry and package ids.
//! Port of `C3LegacyStringId` (C3_CORE_DLL/include/c3_hash.h).

/// Hash of the raw string (no normalization), at most 256 bytes.
pub fn legacy_string_id(text: &str) -> u32 {
    let mut raw = [0u8; 280];
    let src = text.as_bytes();
    let n = src.iter().take(256).position(|&c| c == 0).unwrap_or(src.len().min(256));
    raw[..n].copy_from_slice(&src[..n]);
    let mut words = [0u32; 70];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap());
    }

    let mut count = 0;
    while count < 64 && words[count] != 0 {
        count += 1;
    }
    words[count] = 0x9BE7_4448;
    words[count + 1] = 0x66F4_2C48;
    count += 2;

    let mut v: u32 = 0xF4FA_8928;
    let mut x: u32 = 0x37A8_470E;
    let mut y: u32 = 0x7758_B42B;
    for &w in &words[..count] {
        v = v.rotate_left(1);
        let ebx = 0x267B_0B11 ^ v;
        x ^= w;
        y ^= w;

        let edx = (ebx.wrapping_add(y) | 0x0204_0801) & 0xBFEF_7FDF;
        let p1 = x as u64 * edx as u64;
        let (lo, hi) = (p1 as u32, (p1 >> 32) as u32);
        let first = lo as u64 + hi as u64 + (hi != 0) as u64;
        let eax = (first as u32).wrapping_add((first >> 32) as u32);

        let edx = (ebx.wrapping_add(x) | 0x0080_4021) & 0x7DFE_FBFF;
        x = eax;
        let p2 = y as u64 * edx as u64;
        let (lo, hi) = (p2 as u32, (p2 >> 32) as u32);
        let (sum, c1) = lo.overflowing_add(hi.wrapping_add(hi));
        let (mut sum, c2) = sum.overflowing_add(hi >> 31);
        if c1 || c2 {
            sum = sum.wrapping_add(2);
        }
        y = sum;
    }
    x ^ y
}

/// Normalize a client path the way the client does: lowercase ASCII, `\` → `/`.
pub fn normalize_path(path: &str) -> String {
    path.chars()
        .map(|c| if c == '\\' { '/' } else { c.to_ascii_lowercase() })
        .collect()
}

/// Id of a client file path as stored in WDF indexes.
pub fn path_id(path: &str) -> u32 {
    legacy_string_id(&normalize_path(path))
}
