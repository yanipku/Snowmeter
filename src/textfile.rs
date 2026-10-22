use encoding_rs::{Encoding, WINDOWS_1252};
use std::fs::File;
use std::io::Read;
use std::path::Path;

// Real skins are a few KB. Refusing huge files stops a hostile "skin"
// from making the program load gigabytes into memory.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

pub fn read_text(path: &Path) -> Result<String, String> {
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

fn decode(bytes: &[u8], path: &Path) -> String {
    // 1. A byte order mark (BOM) says exactly which encoding the file uses:
    //    EF BB BF = UTF-8, FF FE = UTF-16 little-endian, FE FF = UTF-16 big-endian
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, had_errors) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        if had_errors {
            eprintln!(
                "{}: some characters were not valid {} and were replaced",
                path.display(),
                encoding.name()
            );
        }
        return text.into_owned();
    }

    // 2. No BOM: plain UTF-8, if the bytes are valid UTF-8
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    // 3. Otherwise assume Windows ANSI, which is what Rainmeter does for files without a BOM
    let (text, _) = WINDOWS_1252.decode_without_bom_handling(bytes);
    text.into_owned()
}