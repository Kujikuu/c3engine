//! TQ ini tables (GBK text).
//!
//! - [`Ini`]: generic sections of `key=value` (keys before any section go in section "").
//! - [`PathTable`]: flat `id=path` tables (`3dobj.ini`, `3dmotion.ini`, `3dtexture.ini`).
//! - [`PartTable`]: `[id]` sections with `Part=N` and per-part `MeshN`/`TextureN`/blend keys
//!   (`armor.ini`, `weapon.ini`, `armet.ini`, `head.ini`, `mount.ini`).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Section {
    /// First value for `key` (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn get_parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key)?.trim().parse().ok()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ini {
    pub sections: Vec<Section>,
}

impl Ini {
    pub fn parse(data: &[u8]) -> Self {
        let text = encoding_rs::GBK.decode_without_bom_handling(data).0;
        let mut sections = vec![Section::default()];
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
                sections.push(Section {
                    name: name.trim().to_owned(),
                    entries: Vec::new(),
                });
            } else if let Some((k, v)) = line.split_once('=') {
                sections
                    .last_mut()
                    .unwrap()
                    .entries
                    .push((k.trim().to_owned(), v.trim().to_owned()));
            }
        }
        if sections[0].entries.is_empty() {
            sections.remove(0);
        }
        Self { sections }
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

/// `id=path` table. Ids are numeric; leading zeros are insignificant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathTable {
    pub paths: BTreeMap<u64, String>,
}

impl PathTable {
    pub fn parse(data: &[u8]) -> Self {
        let ini = Ini::parse(data);
        let mut paths = BTreeMap::new();
        for s in &ini.sections {
            for (k, v) in &s.entries {
                if let Ok(id) = k.parse::<u64>() {
                    // First definition wins, as in the client's map insert.
                    paths.entry(id).or_insert_with(|| v.clone());
                }
            }
        }
        Self { paths }
    }

    pub fn get(&self, id: u64) -> Option<&str> {
        self.paths.get(&id).map(String::as_str)
    }
}

/// One mesh/texture pair of an equipment look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Id in `3dobj.ini`.
    pub mesh: u64,
    /// Id in `3dtexture.ini`.
    pub texture: u64,
    pub mix_texture: u64,
    pub mix_option: u32,
    /// D3D source/destination blend factors.
    pub src_blend: u32,
    pub dst_blend: u32,
    pub material: String,
}

/// `[id]` → parts table for equipment looks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartTable {
    pub looks: BTreeMap<u64, Vec<Part>>,
}

impl PartTable {
    pub fn parse(data: &[u8]) -> Self {
        let ini = Ini::parse(data);
        let mut looks = BTreeMap::new();
        for s in &ini.sections {
            let Ok(id) = s.name.parse::<u64>() else {
                continue;
            };
            let count: usize = s.get_parsed("Part").unwrap_or(0);
            let parts = (0..count)
                .filter_map(|i| {
                    Some(Part {
                        mesh: s.get_parsed(&format!("Mesh{i}"))?,
                        texture: s.get_parsed(&format!("Texture{i}")).unwrap_or(0),
                        mix_texture: s.get_parsed(&format!("MixTex{i}")).unwrap_or(0),
                        mix_option: s.get_parsed(&format!("MixOpt{i}")).unwrap_or(0),
                        src_blend: s.get_parsed(&format!("Asb{i}")).unwrap_or(5),
                        dst_blend: s.get_parsed(&format!("Adb{i}")).unwrap_or(6),
                        material: s
                            .get(&format!("Material{i}"))
                            .unwrap_or("default")
                            .to_owned(),
                    })
                })
                .collect();
            looks.entry(id).or_insert(parts);
        }
        Self { looks }
    }

    pub fn get(&self, id: u64) -> Option<&[Part]> {
        self.looks.get(&id).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_and_part_tables() {
        let t = PathTable::parse(
            b"0002000000=c3/mesh/002000000.c3\r\n2=a.c3\r\n2=dup.c3\r\n// note\r\n",
        );
        assert_eq!(t.get(2000000), Some("c3/mesh/002000000.c3"));
        assert_eq!(t.get(2), Some("a.c3"), "first definition wins");

        let p = PartTable::parse(
            b"[410000]\r\nPart=1\r\nMesh0=410000\r\nTexture0=410006\r\nAsb0=5\r\nAdb0=6\r\n",
        );
        let parts = p.get(410000).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!((parts[0].mesh, parts[0].texture), (410000, 410006));
    }
}
