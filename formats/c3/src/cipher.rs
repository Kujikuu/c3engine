//! "XorRot": TQ's data-file cipher. A 128-byte key comes from the MSVC CRT `rand()`
//! LCG seeded with `srand(seed)`; byte `i` is XORed with `key[i % 128]` then rotated
//! right by `i % 8`. Ports of the loaders in `3DRoleData.cpp` (Monster, UserHelpInfo),
//! `ItemData.cpp` (itemtype), `MapDestination5065.cpp`, `Hero.cpp` (levexp).
//! Decrypted payloads are plain CRLF text.
//!
//! The engine never encrypts its own files (see docs/DECISIONS.md): [`decrypt`] is for
//! importing TQ data, [`encrypt`] only for exporting back to the original 5065 client.

/// Seed for `itemtype.dat`, `Monster.dat`, `MapDestination.dat`, `UserHelpInfo.dat`.
pub const SEED_DATA: u32 = 0x2537;
/// Seed for `levexp.dat`.
pub const SEED_LEVEXP: u32 = 0x4D2;

fn key(seed: u32) -> [u8; 128] {
    let mut state = seed;
    let mut key = [0u8; 128];
    for k in &mut key {
        state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
        *k = ((state >> 16) & 0x7FFF) as u8;
    }
    key
}

pub fn decrypt(data: &[u8], seed: u32) -> Vec<u8> {
    let key = key(seed);
    data.iter()
        .enumerate()
        .map(|(i, &b)| (b ^ key[i % 128]).rotate_right((i % 8) as u32))
        .collect()
}

pub fn encrypt(plain: &[u8], seed: u32) -> Vec<u8> {
    let key = key(seed);
    plain
        .iter()
        .enumerate()
        .map(|(i, &b)| b.rotate_left((i % 8) as u32) ^ key[i % 128])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_matches_crt_rand() {
        // MSVC: srand(1); rand() == 41.
        assert_eq!(key(1)[0], 41);
        let plain = b"Amount=1\r\n1 Item ~ \r\n".repeat(20);
        assert_eq!(decrypt(&encrypt(&plain, SEED_DATA), SEED_DATA), plain);
    }
}
