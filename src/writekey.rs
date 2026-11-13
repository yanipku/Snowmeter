/*  Changes one key in INI text the way Rainmeter's !WriteKeyValue does (IniFile.cpp
    ApplyWriteKey) and leaves every other byte alone: comments, blank line and order stay. */

// one line of the text: where it starts, where its content ends, and where the text line starts
struct Line {
    begin: usize,
    content_end: usize,
    end: usize,
}

// splits text into lines. a line ends with \r\n, \n or \r, or at the end of the text
fn lines(text: &str) ->Vec<Line> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let content_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\r' || b == b'\n')
            .map_or(bytes.len(), |i| pos + i);
        let mut end = content_end;
        if end < bytes.len() {
            end += 1;
            if bytes[content_end] == b'\r' && bytes.get(end) == Some(&b'\n') {
                end += 1;
            }
        }
        found.push(Line { begin: pos, content_end, end });
        pos = end;
    }
    found
}

//Rainmeter trims every character up to the space(tabs, control characters) from file text
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

// the name in a [Section] line: what follows '[' up to the first ']' (or the end), trimmed
fn section_name(line: &str) -> Option<&str> {
    let rest = trim(line).strip_prefix('[')?;
    Some(trim(rest.split_once(']').map_or(rest, |(name, _)| name)))
}

// Names ignore case
fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

// Set key=value in [section] and returns the new text
pub fn write_key(text: &str, section: &str, key: &str, value: &str) -> String {
    // Names given by the skin lose spaces at both ends, like Rainmeter's TrimRequest
    let (section, key) = (section.trim_matches(' '), key.trim_matches(' '));
    // New lines end like the file's own; a file without CRLF gets Linux's \n
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines = lines(text);
    let content = |l: &Line| &text[l.begin..l.content_end];
    let mut out = text.to_string();

    let Some(header) = lines.iter().position(|l| section_name(content(l)).is_some_and(|n| same(n, section)))
    else {
        // No such section: add it at the end of the file, after a blank line
        if !out.is_empty() && !out.ends_with(['\r', '\n']) {
            out.push_str(eol); // end the last line first
            out.push_str(eol);
        } else if lines.last().is_some_and(|l| !content(l).is_empty()) {
            out.push_str(eol);
        }
        out.push_str(&format!("[{section}]{eol}{key}={value}{eol}"));
        return out;
    };
    // The section end where the next one starts, or at the end of the text
    let end = lines[header + 1..]
        .iter()
        .position(|l| section_name(content(l)).is_some())
        .map_or(lines.len(), |i| header + 1 + i);
    // The key's line: not a ;comment, with the key before its first '='
    let found = (header + 1..end).find(|&i| {
        let line = trim(content(&lines[i]));
        !line.starts_with(';') && line.split_once('=').is_some_and(|(k, _)| same(trim(k), key))
    });

    match found {
        Some(i) => {
            // only the value changes: the key keeps its spelling, the line keeps its ending
            let line = &lines[i];
            let equals = line.begin + content(line).find("=").unwrap() + 1; // found above
            let ending = if line.end > line.content_end { &text[line.content_end..line.end] } else { eol };
            out.replace_range(equals..line.end, &format!("{value}{ending}"));
        }
        None => {
            // After the section's last non-blank line, so blank lines stay between sections
            // Step back over blank llines and comments to the section's last key
            let mut at = end;
            while at > header + 1
                && matches!(trim(content(&lines[at - 1])), l if l.is_empty() || l.starts_with(';')) {
                    at -= 1;
            }
            /*  comment right under thet key (no blank line between) belong to it: write after them.
                comment after a blank line belong to the next section, like a ";; Measures" banner. */
            while at < end && trim(content(&lines[at])).starts_with(';') {
                at += 1;
            }
            let offset = lines.get(at).map_or(text.len(), |l| l.begin);
            let mut added = String::new();
            if offset > 0 && !text[..offset].ends_with(['\r', '\n']) {
                added.push_str(eol); // the line before had no line break
            }
            added.push_str(&format!("{key}={value}{eol}"));
            out.insert_str(offset, &added);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_value_changes() {
        // key spelling, the space before '=', other lines and CRLF ending stay
        let text = "; my skin\r\n[Variables]\r\nmyColor = 255,0,0 ; old\r\nSize=10\r\n";
        assert_eq!(
            write_key(text, "variables", "MYCOLOR", "0,255,0"),
            "; my skin\r\n[Variables]\r\nmyColor =0,255,0\r\nSize=10\r\n"
        );
        // A commented-out key isn't the key
        assert_eq!(write_key("[V]\n;A=1\nA=2\n", "V", "A", "3"), "[V]\n;A=1\nA=3\n");
        // A last line without a line break gets one
        assert_eq!(write_key("[V]\nA=1", "V", "A", "2"), "[V]\nA=2\n");
    }

    #[test]
    fn missing_keys_and_sections_are_added() {
        //key: at the end of its section, before the blank line
        assert_eq!(write_key("[V]\nA=1\n\n[W]\nB=2\n", "v", "C", "3"), "[V]\nA=1\nC=3\n\n[W]\nB=2\n");
        // A note right under the last key stays with it; a banner after a blank line stays with [W]
        assert_eq!(write_key("[V]\nA=1\n; note\n\n[W]\n", "V", "C", "3"), "[V]\nA=1\n; note\nC=3\n\n[W]\n");
        assert_eq!(write_key("[V]\nA=1\n\n;; W\n[W]\n", "V", "C", "3"), "[V]\nA=1\nC=3\n\n;; W\n[W]\n");
        assert_eq!(write_key("[V]\nA=1", "V", "B", "2"), "[V]\nA=1\nB=2\n");
        //section: at the end of the file after a blank line: names lose outer spaces
        assert_eq!(
            write_key("[V]\r\nA=1\r\n", " New ", " K ", "v"),
            "[V]\r\nA=1\r\n\r\n[New]\r\nK=v\r\n"
        );
        assert_eq!(write_key("", "V", "A", "1"), "[V]\nA=1\n");
    }
}