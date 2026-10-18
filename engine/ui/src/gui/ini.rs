//! The INI text TQ's UI files are written in: `[section]` headers (a `//` comment may follow
//! them), `key=value` lines, and `//` or `;` comment lines. Text may be GBK; only keys and
//! values are read, so other bytes pass through as replacement characters.

/// One `[section]` and its keys, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub keys: Vec<(String, String)>,
}

impl Section {
    /// The value of `key` (case-insensitive), the first one when it repeats.
    pub fn get(&self, key: &str) -> Option<&str> {
        (self.keys.iter())
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn int(&self, key: &str) -> Option<i32> {
        self.get(key)?.trim().parse().ok()
    }
}

/// The sections of `bytes`, in file order.
pub fn parse(bytes: &[u8]) -> Vec<Section> {
    let text = String::from_utf8_lossy(bytes);
    let mut sections: Vec<Section> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((name, _)) = rest.split_once(']') {
                sections.push(Section {
                    name: name.trim().to_owned(),
                    keys: Vec::new(),
                });
            }
            continue;
        }
        let (Some(section), Some((key, value))) = (sections.last_mut(), line.split_once('='))
        else {
            continue;
        };
        // A value may carry a trailing `//` comment.
        let value = value.split_once("//").map_or(value, |(v, _)| v);
        section
            .keys
            .push((key.trim().to_owned(), value.trim().to_owned()));
    }
    sections
}

/// The `parent-control` ids a section is named by (`[130-1019]`).
pub fn ids(name: &str) -> Option<(u32, u32)> {
    let (parent, control) = name.split_once('-')?;
    Some((parent.trim().parse().ok()?, control.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_keys_and_comments() {
        let text =
            b"// head\n[0-130]\nx=0\ny=627 // bottom\n;w=1\n[130-1019] //IDC_MAIN_EXP\nw=398\n";
        let sections = parse(text);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].int("y"), Some(627));
        assert_eq!(sections[0].get("w"), None);
        assert_eq!(ids(&sections[1].name), Some((130, 1019)));
        assert_eq!(sections[1].int("W"), Some(398));
    }
}
