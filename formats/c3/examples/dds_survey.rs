//! Survey DDS pixel formats used across the client archives.
use c3_formats_tq::wdf::Wdf;
use std::collections::BTreeMap;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or(r"D:\Conquer2\Tools\eosource\client5065".into());
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for name in ["c3.wdf", "data.wdf"] {
        let a = Wdf::open(std::path::Path::new(&dir).join(name)).unwrap();
        for e in &a.entries {
            let b = a.entry_bytes(e);
            if b.len() < 128 || &b[..4] != b"DDS " {
                let tag = if b.len() >= 18 && (b[2] == 2 || b[2] == 10) {
                    "tga?"
                } else {
                    "other"
                };
                *kinds.entry(format!("{name} {tag}")).or_default() += 1;
                continue;
            }
            let flags = u32::from_le_bytes(b[80..84].try_into().unwrap());
            let fourcc = &b[84..88];
            let bits = u32::from_le_bytes(b[88..92].try_into().unwrap());
            let mips = u32::from_le_bytes(b[28..32].try_into().unwrap());
            let k = if flags & 4 != 0 {
                String::from_utf8_lossy(fourcc).into_owned()
            } else {
                let rm = u32::from_le_bytes(b[92..96].try_into().unwrap());
                let am = u32::from_le_bytes(b[104..108].try_into().unwrap());
                format!("rgb{bits} r={rm:#x} a={am:#x}")
            };
            *kinds
                .entry(format!("{name} {k} mips={}", mips.max(1) > 1))
                .or_default() += 1;
        }
    }
    for (k, v) in kinds {
        println!("{v:6} {k}");
    }
}
