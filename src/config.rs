use crate::ini::Ini;
use crate::paths;
use crate::textfile;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 6;

// Where the settings live. If ~/Rainmeter/Rainmeter.ini exists we use it, so a Rainmeter
// setup copied over from Windows just works. Otherwise we use our own
// ~/Snowmeter/Snowmeter.ini. The folder also holds the default Skins folder.
pub struct Config {
    pub dir: PathBuf,          // ~/Rainmeter or ~/Snowmeter
    pub file: PathBuf,         // the .ini file inside it
    pub section: &'static str, // "Rainmeter" or "Snowmeter": the section that holds SkinPath=
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn find_config() -> Config {
    let home = home_dir();
    let rainmeter = home.join("Rainmeter");
    let file = rainmeter.join("Rainmeter.ini");
    if file.is_file() {
        return Config { dir: rainmeter, file, section: "Rainmeter" };
    }
    let dir = home.join("Snowmeter");
    Config { file: dir.join("Snowmeter.ini"), dir, section: "Snowmeter" }
}

impl Config {
    // The first run makes Snowmeter.ini with an empty [Snowmeter] section.
    // (A Rainmeter.ini is never created: it's only used when it already exists.)
    pub fn create_if_missing(&self) -> Result<(), String> {
        if self.file.exists() {
            return Ok(());
        }
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Cannot create {}: {}", self.dir.display(), e))?;
        fs::write(&self.file, format!("[{}]\n", self.section))
            .map_err(|e| format!("Cannot write {}: {}", self.file.display(), e))
    }
}

// Lets "~" and "~/something" work even when the shell didn't expand them
pub fn expand_home(s: &str) -> PathBuf {
    if s == "~" || s.starts_with("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(s[1..].trim_start_matches('/'));
        }
    }
    PathBuf::from(s)
}

// Reads SkinPath= from the config file's own section
fn configured_dir(cfg: &Config) -> Option<PathBuf> {
    // read_text understands UTF-16, which is how Rainmeter on Windows saves Rainmeter.ini
    let text = textfile::read_text(&cfg.file).ok()?;
    let ini = Ini::parse(&text);
    let value = ini.section(cfg.section)?.options.get("skinpath")?.clone();
    let path = expand_home(&value);
    if !path.is_absolute() {
        // A Windows path like C:\Users\...\Skins\ also lands here
        eprintln!(
            "{}: SkinPath must be an absolute path (or start with ~/), ignoring it",
            cfg.file.display()
        );
        return None;
    }
    Some(path)
}

// Picks the skins folder and says where the choice came from
pub fn resolve_skins_dir(cli: Option<String>, cfg: &Config) -> (PathBuf, &'static str) {
    if let Some(dir) = cli {
        return (expand_home(&dir), "--skins-dir");
    }
    if let Some(dir) = env::var_os("RAINMETER_SKINS").filter(|v| !v.is_empty()) {
        return (PathBuf::from(dir), "RAINMETER_SKINS");
    }
    if let Some(dir) = configured_dir(cfg) {
        return (dir, "config file");
    }
    (cfg.dir.join("Skins"), "default")
}

// Creates the folder if needed and returns its absolute, symlink-free path
pub fn ensure_skins_dir(dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("Cannot create skins folder {}: {}", dir.display(), e))?;
    dir.canonicalize()
        .map_err(|e| format!("Cannot open skins folder {}: {}", dir.display(), e))
}

// Remembers a skins folder for future runs by writing SkinPath= into the config file.
// Only that one line changes: the rest of the file (a real Rainmeter.ini has a lot) is kept.
pub fn save_skins_dir(dir: &Path, cfg: &Config) -> Result<PathBuf, String> {
    let root = ensure_skins_dir(dir)?;
    let path = root.to_string_lossy().to_string();
    // A line break in the path would corrupt the file
    if path.contains('\n') || path.contains('\r') {
        return Err("That folder name contains a line break and can't be saved".to_string());
    }
    // If the file exists but can't be read, stop: writing would wipe what's in it
    let old = if cfg.file.exists() { textfile::read_text(&cfg.file)? } else { String::new() };
    let new = set_option(&old, cfg.section, "SkinPath", &path);
    fs::create_dir_all(&cfg.dir)
        .map_err(|e| format!("Cannot create {}: {}", cfg.dir.display(), e))?;
    fs::write(&cfg.file, new)
        .map_err(|e| format!("Cannot write {}: {}", cfg.file.display(), e))?;
    Ok(cfg.file.clone())
}

// The section name if this line is a [Header], else None
fn header_name(line: &str) -> Option<&str> {
    let t = line.trim();
    (t.starts_with('[') && t.ends_with(']')).then(|| t[1..t.len() - 1].trim())
}

// Sets key=value inside [section] and leaves every other line alone. Replaces the key's
// line if the section has one, else adds it after the section's last line, else adds
// the section at the end of the file. Names are compared case-insensitively, like INI reading.
fn set_option(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let new_line = format!("{}={}", key, value);

    let start = lines
        .iter()
        .position(|l| header_name(l).is_some_and(|n| n.eq_ignore_ascii_case(section)));
    match start {
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new()); // a blank line before the new section
            }
            lines.push(format!("[{}]", section));
            lines.push(new_line);
        }
        Some(start) => {
            // The section ends where the next [Header] starts, or at the end of the file
            let end = lines[start + 1..]
                .iter()
                .position(|l| header_name(l).is_some())
                .map_or(lines.len(), |i| start + 1 + i);
            let existing = (start + 1..end).find(|&i| {
                lines[i]
                    .split_once('=')
                    .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
            });
            match existing {
                Some(i) => lines[i] = new_line,
                None => {
                    // After the section's last non-blank line, so blank lines stay between sections
                    let mut at = end;
                    while at > start + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, new_line);
                }
            }
        }
    }
    lines.join("\n") + "\n"
}

// Every .ini inside a config folder, as a path relative to the skins folder
pub fn list_skins(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    found.sort_by_key(|s| s.to_lowercase());
    found
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        // DirEntry::file_type does not follow symlinks, so loops can't trap us
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_dir() {
            if !name.starts_with('@') {
                // skip @Resources and similar
                walk(root, &entry.path(), depth + 1, out);
            }
        } else if kind.is_file() && depth > 0 && name.to_lowercase().ends_with(".ini") {
            if let Ok(rel) = entry.path().strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

// Turns what the user typed into the absolute path of a skin file
pub fn resolve_skin(arg: &str, root: &Path) -> Result<PathBuf, String> {
    // 1. A real path on disk wins (relative to where you ran the command, or absolute)
    let direct = Path::new(arg);
    if direct.is_file() {
        return direct
            .canonicalize()
            .map_err(|e| format!("Cannot resolve {}: {}", arg, e));
    }

    // 2. A name inside the skins folder
    find_in_skins(arg, root)
}

// A skin file inside the skins folder: Config\Skin\File.ini, any capitalization.
// Never anything outside it, so skins' own bangs can use this safely.
pub fn find_in_skins(arg: &str, root: &Path) -> Result<PathBuf, String> {
    let found = paths::find_file(root, arg)
        .ok_or_else(|| format!("Skin '{}' not found (looked in {})", arg, root.display()))?;
    let found = found
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {}: {}", arg, e))?;

    // A name like ..\..\something must not escape the skins folder
    if !found.starts_with(root) {
        return Err(format!("'{}' points outside the skins folder", arg));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_option_replaces_only_its_line() {
        let text = "[Rainmeter]\nLanguage=1033\nskinpath=/old\n\n[illustro\\Clock]\nActive=1\n";
        assert_eq!(
            set_option(text, "rainmeter", "SkinPath", "/new"),
            "[Rainmeter]\nLanguage=1033\nSkinPath=/new\n\n[illustro\\Clock]\nActive=1\n"
        );
    }

    #[test]
    fn set_option_adds_key_or_section() {
        // Key missing: added at the end of its section, before the blank line
        assert_eq!(
            set_option("[Snowmeter]\n\n[Other]\nA=1\n", "Snowmeter", "SkinPath", "/s"),
            "[Snowmeter]\nSkinPath=/s\n\n[Other]\nA=1\n"
        );
        // Section missing: added at the end of the file
        assert_eq!(
            set_option("[Other]\nA=1\n", "Snowmeter", "SkinPath", "/s"),
            "[Other]\nA=1\n\n[Snowmeter]\nSkinPath=/s\n"
        );
        // Empty file
        assert_eq!(set_option("", "Snowmeter", "SkinPath", "/s"), "[Snowmeter]\nSkinPath=/s\n");
    }
}