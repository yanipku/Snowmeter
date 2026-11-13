use encoding_rs::{Encoding, UTF_16LE, UTF_8, WINDOWS_1252};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use crate::log::{skin_log, Level};

// Real skins are a few KB. Refusing huge files stops a hostile "skin"
// from making the program load gigabytes into memory.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Utf8 { bom: bool },
    Utf16Le,
    Utf16Be,
    Ansi, // windows-1252, Rainmeter's guess for files that aren't UTF-8
}

pub fn read_text(path: &Path) -> Result<String, String> {
    read_text_kind(path).map(|(text, _)| text)
}

// Like read_text, and also says how the file was stored (for write_text) 
pub fn read_text_kind(path: &Path) -> Result<(String, Kind), String> {
    let file = File::open(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;

    // Read at most one byte more than the limit, so we can tell "too big" from "just fits"
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!(
            "{} is larger than {} MiB, refusing to load it",
            path.display(),
            MAX_BYTES / (1024 * 1024)
        ));
    }

    Ok(decode(&bytes, path))
}

fn decode(bytes: &[u8], path: &Path) -> (String, Kind) {
    // 1. A byte order mark (BOM) says exactly which encoding the file uses:
    //    EF BB BF = UTF-8, FF FE = UTF-16 little-endian, FE FF = UTF-16 big-endian
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, had_errors) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        if had_errors {
            skin_log(Level::Warning, &format!("{}: some characters were not valid {} and were replaced", path.display(), encoding.name()));
        }
        let kind = if encoding == UTF_8 {
            Kind::Utf8 { bom: true }
        } else if encoding == UTF_16LE {
            Kind::Utf16Le
        } else {
            Kind::Utf16Be
        };
        return (text.into_owned(), kind);
    }

    // 2. No BOM: plain UTF-8, if the bytes are valid UTF-8
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_string(), Kind::Utf8 { bom: false });
    }

    // 3. Otherwise assume Windows ANSI, which is what Rainmeter does for files without a BOM
    let (text, _) = WINDOWS_1252.decode_without_bom_handling(bytes);
    (text.into_owned(), Kind::Ansi)
}

/*Writes text back the way read_text_kind found it stored. UTF_16 is written by hand:
encoding_rs only decodes it (its encoder for UTF-16 gives UTF-8). */
pub fn write_text(path: &Path, text: &str, kind: Kind) -> Result<(), String> {
    let bytes: Vec<u8> = match kind {
        Kind::Utf8 { bom } => {
            let mut b= if bom { vec![0xEF, 0xBB, 0xBF] } else { Vec::new() };
            b.extend_from_slice(text.as_bytes());
            b
        }
        Kind::Utf16Le => [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(u16::to_le_bytes)).collect(),
        Kind::Utf16Be => [0xFE, 0xFF].into_iter().chain(text.encode_utf16().flat_map(u16::to_be_bytes)).collect(),
        Kind::Ansi => {
            //A character Windows-1252 doesn't have would be written as "&#9731;": refuse instead
            let (b, _, unmapple) = WINDOWS_1252.encode(text);
            if unmapple {
                return Err(format!("{}: some characters can't be stored in its ANSI encoding", path.display()));
            }
            b.into_owned()
        }
    };
    // A file we couldn't read back would break the skin
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("{} would be larger than {} MiB, not writting it", path.display(), MAX_BYTES / (1024 * 1024)));
    }
    fs::write(path, bytes).map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_keep_their_encoding() {
        let path = std::env::temp_dir().join(format!("snowmeter-test-enc-{}.ini", std::process::id()));
        let text = "[Variables]\r\nName=Café\r\n";
        for kind in [Kind::Utf8 { bom: false }, Kind::Utf8 { bom: true }, Kind::Utf16Le, Kind::Utf16Be, Kind::Ansi] {
            write_text(&path, text, kind).unwrap();
            assert_eq!(read_text_kind(&path).unwrap(), (text.to_string(), kind));
        }
        // ANSI can't hold every character
        assert!(write_text(&path, "Snow \u{2603}", Kind::Ansi).is_err());
        fs::remove_file(&path).unwrap();
    }
}