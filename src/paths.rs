use std::fs;
use std::path::{Path, PathBuf};

// Finds a file like Windows would: backslashes allowed, names case-insensitive.
pub fn find_file(base: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim().replace('\\', "/");
    let mut current = if name.starts_with('/') {
        PathBuf::from("/")
    } else {
        base.to_path_buf()
    };

    for part in name.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            current.pop();
            continue;
        }
        let exact = current.join(part);
        if exact.exists() {
            current = exact;
            continue;
        }
        // Not found as spelled: search this directory ignoring case
        let found = fs::read_dir(&current)
            .ok()?
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        current = found.path();
    }

    if current.is_file() {
        Some(current)
    } else {
        None
    }
}