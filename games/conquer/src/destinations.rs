//! The auto-path destinations (`ini/MapDestination.dat`, CMapDestinationManager5065::Load):
//! an ini of sections, each byte XORed with a 128-byte key from MSVC `rand()` seeded 0x2537
//! and rotated left by its place mod 8. A section with a `parentid` is a place under that
//! heading; `~` stands for a space.

use c3_ui::Destination;

/// The key's seed (CMapDestinationManager5065_Load @ 0x004B369D).
const SEED: u32 = 0x2537;

/// The 128-byte key: `rand() % 256` after `srand(SEED)`, as MSVC's runtime counts.
fn key() -> [u8; 128] {
    let mut seed = SEED;
    std::array::from_fn(|_| {
        seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((seed >> 16) & 0x7FFF) as u8
    })
}

/// The file's text.
fn decode(bytes: &[u8]) -> String {
    let key = key();
    let plain: Vec<u8> = (bytes.iter().enumerate())
        .map(|(i, b)| (b ^ key[i % 128]).rotate_right((i % 8) as u32))
        .collect();
    String::from_utf8_lossy(&plain).into_owned()
}

/// The destinations the file lists, in its order.
pub fn read(bytes: &[u8]) -> Vec<Destination> {
    let mut out = Vec::new();
    let mut current: Option<Destination> = None;
    for line in decode(bytes).lines().map(str::trim) {
        if line.len() >= 2 && line.starts_with('[') && line.ends_with(']') {
            out.extend(current.replace(Destination::default()));
            continue;
        }
        let Some(d) = current.as_mut() else {
            continue;
        };
        if line.starts_with([';', '#']) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        let number = || value.parse::<i32>().unwrap_or(0);
        let text = || value.replace('~', " ");
        match key.as_str() {
            "id" => d.id = number().max(0) as u32,
            "mapid" => d.map = number().max(0) as u32,
            "npcid" => d.npc = number().max(0) as u32,
            "parentid" => d.parent = number().max(0) as u32,
            "radius" => d.radius = number().max(0) as u32,
            "posx" => d.cell.0 = number(),
            "posy" => d.cell.1 = number(),
            "mark" => d.mark = text(),
            "title" => d.title = text(),
            _ => {}
        }
    }
    out.extend(current);
    // A heading's places are on its map.
    let maps: Vec<(u32, u32)> = (out.iter())
        .filter(|d| d.parent == 0)
        .map(|d| (d.id, d.map))
        .collect();
    for d in out.iter_mut().filter(|d| d.parent != 0 && d.map == 0) {
        d.map = (maps.iter())
            .find(|(id, _)| *id == d.parent)
            .map_or(0, |(_, m)| *m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file as the client's tool writes it (the decoding undone).
    fn encode(text: &str) -> Vec<u8> {
        let key = key();
        (text.bytes().enumerate())
            .map(|(i, b)| b.rotate_left((i % 8) as u32) ^ key[i % 128])
            .collect()
    }

    #[test]
    fn headings_and_places_read_from_the_coded_file() {
        let text = "//Birth Village\n[1010-1]\ntitle=New~Skill~Learning\nid=01\nmapid=1010\n\
                    posx=97\nposy=45\nradius=2\nnpcid=425\nmark=Old~General\\bYang.\n\
                    [1010-3]\ntitle=Feature~Introduction\nid=03\nmapid=1010\n\
                    [1010-3-1]\ntitle=Class~Introduction\nparentid=03\nposx=70\nposy=40\n";
        let read = read(&encode(text));
        assert_eq!(read.len(), 3);
        assert_eq!(
            (read[0].title.as_str(), read[0].cell, read[0].npc),
            ("New Skill Learning", (97, 45), 425)
        );
        assert_eq!(read[0].mark, "Old General\\bYang.");
        assert_eq!(
            (read[2].parent, read[2].map, read[2].cell),
            (3, 1010, (70, 40))
        );
    }
}
