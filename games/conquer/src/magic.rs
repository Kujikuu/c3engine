//! Conquer's skill media: `MagicType.dat` (plain, fixed-size records, one per skill level)
//! names the effect shown on each one a skill reaches and the sound of its cast.

use crate::Content;
use std::collections::HashMap;

/// Size of one `MagicType.dat` record.
const RECORD: usize = 1584;
/// Where a record keeps its fields (text fields are 64 bytes, NUL-terminated).
const TARGET_EFFECT: usize = 1120;
const SOUND: usize = 1184;
const TEXT: usize = 64;

/// The effect and sound of a skill.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MagicMedia {
    pub effect: Option<String>,
    pub sound: Option<String>,
}

/// Skill type → its media, from the first level's record of each.
pub(crate) fn parse(bytes: &[u8]) -> HashMap<u32, MagicMedia> {
    let mut media = HashMap::new();
    let Some(count) = word(bytes, 0) else {
        return media;
    };
    // The count, an index of ids, then the records.
    let start = 4 + 4 * count as usize;
    for record in bytes
        .get(start..)
        .unwrap_or_default()
        .as_chunks::<RECORD>()
        .0
    {
        let Some(id) = word(record, 0) else { continue };
        media.entry(id).or_insert_with(|| MagicMedia {
            effect: text(record, TARGET_EFFECT),
            sound: text(record, SOUND),
        });
    }
    media
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A text field, `None` when empty or `NULL`.
fn text(record: &[u8], at: usize) -> Option<String> {
    let field = record.get(at..at + TEXT)?;
    let end = field.iter().position(|&b| b == 0).unwrap_or(TEXT);
    let value = String::from_utf8_lossy(&field[..end]).trim().to_owned();
    (!value.is_empty() && !value.eq_ignore_ascii_case("null")).then_some(value)
}

impl Content {
    /// The effect and sound of skill `skill`.
    pub fn magic_media(&self, skill: u32) -> (Option<&str>, Option<&str>) {
        self.tables
            .magic
            .get(&skill)
            .map_or((None, None), |m| (m.effect.as_deref(), m.sound.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, effect: &str, sound: &str) -> Vec<u8> {
        let mut r = vec![0; RECORD];
        r[..4].copy_from_slice(&id.to_le_bytes());
        r[TARGET_EFFECT..TARGET_EFFECT + effect.len()].copy_from_slice(effect.as_bytes());
        r[SOUND..SOUND + sound.len()].copy_from_slice(sound.as_bytes());
        r
    }

    #[test]
    fn reads_the_first_level_of_each_skill() {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        for id in [1000u32, 1000, 1190] {
            bytes.extend(id.to_le_bytes());
        }
        bytes.extend(record(1000, "Thunder", "sound\\thunder.wav"));
        bytes.extend(record(1000, "Other", "NULL"));
        bytes.extend(record(1190, "zf2-e030", "NULL"));
        let media = parse(&bytes);
        assert_eq!(media[&1000].effect.as_deref(), Some("Thunder"));
        assert_eq!(media[&1000].sound.as_deref(), Some("sound\\thunder.wav"));
        assert_eq!(media[&1190].sound, None);
        assert!(parse(&[]).is_empty());
    }
}
