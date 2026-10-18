//! The INI text TQ's UI files are written in: `[section]` headers (a `//` comment may follow
//! them), `key=value` lines, and `//` or `;` comment lines. Text may be GBK; only keys and
//! values are read, so other bytes pass through as replacement characters. Edits patch the
//! bytes in place ([`set`], [`remove`]), so comments and everything else stay as they were.

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
    let mut sections: Vec<Section> = Vec::new();
    for line in lines(bytes) {
        if let Some(name) = header(line) {
            sections.push(Section {
                name,
                keys: Vec::new(),
            });
            continue;
        }
        let (Some(section), Some(key)) = (sections.last_mut(), key_of(line)) else {
            continue;
        };
        let text = String::from_utf8_lossy(line);
        let value = text.split_once('=').map_or("", |(_, v)| v);
        // A value may carry a trailing `//` comment.
        let value = value.split_once("//").map_or(value, |(v, _)| v);
        section.keys.push((key, value.trim().to_owned()));
    }
    sections
}

/// The `parent-control` ids a section is named by (`[130-1019]`).
pub fn ids(name: &str) -> Option<(u32, u32)> {
    let (parent, control) = name.split_once('-')?;
    Some((parent.trim().parse().ok()?, control.trim().parse().ok()?))
}

/// The file's lines, each with its line ending.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|&b| b == b'\n').collect()
}

/// The section a header line opens, if it is one.
fn header(line: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(line);
    let rest = text.trim().strip_prefix('[')?;
    Some(rest.split_once(']')?.0.trim().to_owned())
}

/// The key a `key=value` line sets, if it is one.
fn key_of(line: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(line);
    let text = text.trim();
    if text.starts_with("//") || text.starts_with(';') || text.starts_with('[') {
        return None;
    }
    Some(text.split_once('=')?.0.trim().to_owned())
}

/// The line ending the file uses.
fn eol(bytes: &[u8]) -> &'static [u8] {
    if bytes.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    }
}

/// The lines of `section` (header included): `start..end` of [`lines`].
fn span(lines: &[&[u8]], section: &str) -> Option<(usize, usize)> {
    let start = (lines.iter()).position(|l| header(l).as_deref() == Some(section))?;
    let end = (start + 1..lines.len())
        .find(|&i| header(lines[i]).is_some())
        .unwrap_or(lines.len());
    Some((start, end))
}

/// `bytes` with `key` of `section` set to `value` (`None`: removed), everything else as it
/// was; a missing section is added at the end.
pub fn set(bytes: &[u8], section: &str, key: &str, value: Option<&str>) -> Vec<u8> {
    let eol = eol(bytes);
    let mut lines: Vec<Vec<u8>> = lines(bytes).into_iter().map(<[u8]>::to_vec).collect();
    let new_line = |v: &str| [format!("{key}={v}").as_bytes(), eol].concat();
    let found = {
        let refs: Vec<&[u8]> = lines.iter().map(Vec::as_slice).collect();
        span(&refs, section)
    };
    let Some((start, end)) = found else {
        if let Some(v) = value {
            if let Some(last) = lines.last_mut()
                && !last.ends_with(b"\n")
            {
                last.extend_from_slice(eol);
            }
            if !lines.is_empty() {
                lines.push(eol.to_vec());
            }
            lines.push([format!("[{section}]").as_bytes(), eol].concat());
            lines.push(new_line(v));
        }
        return lines.concat();
    };
    let line =
        (start + 1..end).find(|&i| key_of(&lines[i]).is_some_and(|k| k.eq_ignore_ascii_case(key)));
    match (line, value) {
        (Some(i), Some(v)) => lines[i] = new_line(v),
        (Some(i), None) => {
            lines.remove(i);
        }
        (None, Some(v)) => {
            // After the section's last key, so blank lines stay before the next section.
            let at = (start + 1..end)
                .rev()
                .find(|&i| key_of(&lines[i]).is_some())
                .map_or(start + 1, |i| i + 1);
            if !lines[at - 1].ends_with(b"\n") {
                lines[at - 1].extend_from_slice(eol);
            }
            lines.insert(at, new_line(v));
        }
        (None, None) => {}
    }
    lines.concat()
}

/// `bytes` without `section` (its header, keys and the lines up to the next section).
pub fn remove(bytes: &[u8], section: &str) -> Vec<u8> {
    let lines = lines(bytes);
    match span(&lines, section) {
        Some((start, end)) => [&lines[..start], &lines[end..]].concat().concat(),
        None => bytes.to_vec(),
    }
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

    #[test]
    fn patches_keep_the_rest_of_the_file() {
        let text = b"//\xd6\xd0 GBK\r\n[0-130]\r\nx=0\r\ny=627\r\n\r\n[130-7] //IDC\r\nw=4\r\n";
        let moved = set(text, "0-130", "y", Some("600"));
        assert_eq!(
            moved,
            b"//\xd6\xd0 GBK\r\n[0-130]\r\nx=0\r\ny=600\r\n\r\n[130-7] //IDC\r\nw=4\r\n"
        );
        let sized = set(&moved, "130-7", "h", Some("9"));
        assert!(sized.ends_with(b"[130-7] //IDC\r\nw=4\r\nh=9\r\n"));
        let added = set(&sized, "130-8", "x", Some("1"));
        assert!(added.ends_with(b"h=9\r\n\r\n[130-8]\r\nx=1\r\n"));
        assert_eq!(
            remove(&added, "130-8"),
            [sized.as_slice(), b"\r\n"].concat()
        );
        let unset = set(text, "0-130", "x", None);
        assert!(unset.starts_with(b"//\xd6\xd0 GBK\r\n[0-130]\r\ny=627\r\n"));
    }
}
