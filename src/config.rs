use crate::ini::Ini;
use crate::log::{log, Level};
use crate::paths;
use crate::position::{eval_formula, Options, ZPos};
use crate::sectionvar::leading_int;
use crate::skin::whole_number;
use crate::textfile;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 6;

/*  Where the settings live. If ~/Rainmeter/Rainmeter.ini exists we use it, so a Rainmeter
    setup copied over from Windows just works. Otherwise we use our own
    ~/Snowmeter/Snowmeter.ini. The folder also holds the default Skins folder. */
#[derive(Clone)]
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
    /*  The first run makes Snowmeter.ini with an empty [Snowmeter] section.
        (A Rainmeter.ini is never created: it's only used when it already exists.) */
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
        log(Level::Warning, "", &format!("{}: SkinPath must be an absolute path (or start with ~/), ignoring it", cfg.file.display()));
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

/*  Remembers a skins folder for future runs by writing SkinPath= into the config file.
    Only that one line changes: the rest of the file (a real Rainmeter.ini has a lot) is kept. */
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

/*  Sets key=value inside [section] and leaves every other line alone. Replaces the key's
    line if the section has one, else adds it after the section's last line, else adds
    the section at the end of the file. Names are compared case-insensitively, like INI reading. */
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

/*  A skin file inside the skins folder: Config\Skin\File.ini, any capitalization.
    Never anything outside it, so skins' own bangs can use this safely. */
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

/*  The .ini files directly inside one config folder, in Rainmeter's order. Active=N in
    the settings file counts from 1 in this list. */
pub fn config_files(root: &Path, config: &str) -> Vec<String> {
    let wanted = config.trim().replace('/', "\\").to_lowercase();
    list_skins(root)
        .into_iter()
        .filter_map(|rel| {
            let rel = rel.replace('/', "\\");
            let (folder, file) = rel.rsplit_once('\\')?;
            (folder.to_lowercase() == wanted).then(|| file.to_string())
        })
        .collect()
}

/*  One folder of the skins tree, as in Rainmeter's Skins menu: the .ini files directly in 
    it, and its subfolders. Only folders with a skin somewhere below them are in the tree. */
pub struct SkinFolder {
    pub name: String,
    pub files: Vec<String>,
    pub folders: Vec<SkinFolder>,
}

// Every skin under the skins folder, as a tree of folders (see tree_from)
pub fn skin_tree(root: &Path) -> Vec<SkinFolder> {
    tree_from(&list_skins(root))
}

/*  Builds the tree from list_skins' paths ("illustro/Clock/Clock.ini"), sorted by name
    ignoring case. A backup folder at top is left out, as in Rainmeter (its installer
    keeps the old version of skins there). */
fn tree_from(skins: &[String]) -> Vec<SkinFolder> {
    let mut top = Vec::new();
    for skin in skins {
        let parts: Vec<&str> = skin.split(['/', '\\']).collect();
        let Some((file, folders)) = parts.split_last() else { continue };
        if folders.first().is_none_or(|f| f.eq_ignore_ascii_case("backup")) {
            continue; // a file right in the skins folder isn't a skin
        }
        add_to_tree(&mut top, folders, file);
    }
    sort_tree(&mut top);
    top
}

// Puts 'file' in the folder that 'folders' names, making the folders on the way as needed
fn add_to_tree(list: &mut Vec<SkinFolder>, folders: &[&str], file: &str) {
    let Some((first, rest)) = folders.split_first() else { return };
    let index = match list.iter().position(|f| f.name == *first) {
        Some(i) => i,
        None => {
            list.push(SkinFolder { name: first.to_string(), files: Vec::new(), folders: Vec::new() });
            list.len() -1
        }
    };
    if rest.is_empty() {
        list[index].files.push(file.to_string());
    } else {
        add_to_tree(&mut list[index].folders, rest, file);
    }
}

fn sort_tree(list: &mut [SkinFolder]) {
    list.sort_by_key(|f| f.name.to_lowercase());
    for folder in list {
        folder.files.sort_by_key(|f| f.to_lowercase());
        sort_tree(&mut folder.folders);
    }
}

/*  Writes Active=N into the config's [section] (0 = not loaded), keeping the rest of the file.
    The section is named like Rainmeter's: [illustro\Clock]. */
pub fn save_active(cfg: &Config, config: &str, index: usize) -> Result<(), String> {
    save_options(cfg, config, &[("Active", index.to_string())])
}

// Writes WindowX= and WindowY= into the config's [section]
pub fn save_position(cfg: &Config, config: &str, x: &str, y: &str) -> Result<(), String> {
    save_options(cfg, config, &[("WindowX", x.to_string()), ("WindowY", y.to_string())])
}

// Writes one setting into the config's [section], such as Draggable=0 after a !Draggable 0
pub fn save_setting(cfg: &Config, config: &str, key: &str, value: &str) -> Result<(), String> {
    save_options(cfg, config, &[(key, value.to_string())])
}

// Sets some keys in a config's [section], reading and writing the file once
pub fn save_options(cfg: &Config, config: &str, options: &[(&str, String)]) -> Result<(), String> {
    let section = config.trim().replace('/', "\\");
    // A ] or a line break in the name would break the file's sections
    if section.contains([']', '\n', '\r']) {
        return Err(format!("Can't save the config name \"{section}\""));
    }
    // As in save_skins_dir: a file that can't be read must not be overwritten
    let mut text = if cfg.file.exists() { textfile::read_text(&cfg.file)? } else { String::new() };
    for (key, value) in options {
        text = set_option(&text, &section, key, value);
    }
    fs::write(&cfg.file, text).map_err(|e| format!("Cannot write {}: {}", cfg.file.display(), e))
}

// A config's window settings, from its [section] in the settings file
#[derive(Debug, Clone)]
pub struct SkinSettings {
    /*  WindowX, WindowY, AnchorX and AnchorY as written, such as "10%R@2" (position.rs reads
        them). A missing one is "0", Rainmeter's default. */
    pub position: Options,
    pub z: ZPos,                // AlwaysOnTop=, default 0 (Normal)
    pub draggable: bool,        // Draggable=, default 1
    pub click_through: bool,    // ClickThrough=, default 0
    pub keep_on_screen: bool,   // KeepOnScreen=, default 1
    pub snap_edges: bool,       // SnapEdges=, default 1
    pub save_position: bool,    // SavePosition=, default 1 (0 = moves aren't saved)
    pub group: String,          // Group=: the config's groups (the skin file can add more)
    pub alpha: u8,              // AlphaValue=, 0 (invisible) to 255 (opaque), default 255
    pub fade_duration: u32,     // FadeDuration=, in ms, default 250 (0 = no fade)
}

/*  Reads a config's window settings, reading the file once. Missing ones get
    Rainmeter's defaults, and so does everything when the file can't be read. */
pub fn read_skin_settings(cfg: &Config, config: &str) -> SkinSettings {
    let text = textfile::read_text(&cfg.file).unwrap_or_default();
    let ini = Ini::parse(&text);
    let section = ini.section(&config.trim().replace('/', "\\"));
    let get = |key: &str| section.and_then(|s| s.options.get(key)).map(|v| v.trim().to_string());
    // Rainmeter's ReadBool: the whole number at the start, 0 = false
    let get_bool = |key: &str, default: bool| get(key).map_or(default, |v| leading_int(&v).unwrap_or(0) != 0);

    // A (formula) is worked out here, once as Rainmeter does when it reads them
    let or_zero = |key: &str| {
        let (text, error) = eval_formula(&get(key).unwrap_or_else(|| "0".into()));
        if let Some(error) = error {
            log(Level::Error, &config.replace('/', "\\"), &error);
        }
        text
    };
    let position = Options {
        x: or_zero("windowx"),
        y: or_zero("windowy"),
        anchor_x: or_zero("anchorx"),
        anchor_y: or_zero("anchory"),
    };
    SkinSettings {
        position,
        z: get("alwaysontop").map_or(ZPos::Normal, |v| ZPos::parse(&v)),
        draggable: get_bool("draggable", true),
        click_through: get_bool("clickthrough", false),
        keep_on_screen: get_bool("keeponscreen", true),
        snap_edges: get_bool("snapedges", true),
        save_position: get_bool("saveposition", true),
        group: get("group").unwrap_or_default(),
        alpha: get("alphavalue").map_or(255, |v| whole_number(&v).clamp(0, 255) as u8),
        fade_duration: get("fadeduration").map_or(250, |v| whole_number(&v).max(0) as u32),
    }
}

// Just the position part of read_skin_settings
pub fn read_position(cfg: &Config, config: &str) -> Options {
    read_skin_settings(cfg, config).position
}

/*  Whether the config already has settings: a [section] with at least one key in it.
    Rainmeter's ActivateSkin aska=s this before it writes Active=, and only a config with
    no settings yet gets the skin's own Default... settings. */
pub fn has_settings(cfg: &Config, config: &str) -> bool {
    let text = textfile::read_text(&cfg.file).unwrap_or_default();
    let ini = Ini::parse(&text);
    ini.section(&config.trim().replace('/', "\\")).is_some_and(|s| !s.options.is_empty())
}

/*  Writes the window settings that are missing from the config's [section], with the
    values `s` has for them (their defaults, since they're missing). Settings already in
    the file are left alone. As in Rainmeter's ReadOptions/WriteOptions, this gives a newly
    loaded skin a full set of settings to edit. */
pub fn save_missing_settings(cfg: &Config, config: &str, s: &SkinSettings) -> Result<(), String> {
    let text = if cfg.file.exists() { textfile::read_text(&cfg.file)? } else { String::new() };
    let ini = Ini::parse(&text);
    let section = ini.section(&config.trim().replace('/', "\\"));
    let has = |key: &str| section.is_some_and(|sec| sec.options.contains_key(&key.to_lowercase()));
    let bool_text = |b: bool| if b { "1" } else { "0" }.to_string();
    let all = [
        ("WindowX", s.position.x.clone()),
        ("WindowY", s.position.y.clone()),
        ("AlwaysOnTop", s.z.number().to_string()),
        ("Draggable", bool_text(s.draggable)),
        ("SnapEdges", bool_text(s.snap_edges)),
        ("ClickThrough", bool_text(s.click_through)),
        ("KeepOnScreen", bool_text(s.keep_on_screen)),
    ];
    let missing: Vec<(&str, String)> = all.into_iter().filter(|(key, _)| !has(key)).collect();
    if missing.is_empty() {
        return Ok(()); // the usual case once a skin has been loaded before: no write
    }
    save_options(cfg, config, &missing)
}

/*  Every config with a valid Active=N: the skin files to load at start-up.
    As in Rainmeter, a number outside 1..=(number of files) is ignored. */
pub fn active_skins(cfg: &Config, root: &Path) -> Vec<PathBuf> {
    let Ok(text) = textfile::read_text(&cfg.file) else { return Vec::new() };
    let ini = Ini::parse(&text);
    // The config folders, each once (list_skins is sorted, so a config's files are together)
    let mut configs: Vec<String> = list_skins(root)
        .iter()
        .filter_map(|rel| Some(rel.replace('/', "\\").rsplit_once('\\')?.0.to_string()))
        .collect();
    configs.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut found = Vec::new();
    for config in configs {
        let Some(active) = ini.section(&config).and_then(|s| s.options.get("active")) else {
            continue;
        };
        let Ok(n) = active.trim().parse::<usize>() else { continue };
        let files = config_files(root, &config);
        if n >= 1 && n <= files.len() {
            if let Ok(path) = find_in_skins(&format!("{config}\\{}", files[n - 1]), root) {
                found.push(path);
            }
        }
    }
    found
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

    #[test]
    fn active_round_trip() {
        // A throwaway skins folder and settings file in the system's temp folder
        let base = env::temp_dir().join(format!("snowmeter-test-{}", std::process::id()));
        let root = base.join("Skins");
        fs::create_dir_all(root.join("Suite/Clock")).unwrap();
        for f in ["b.ini", "A.ini"] {
            fs::write(root.join("Suite/Clock").join(f), "").unwrap();
        }
        let root = root.canonicalize().unwrap();
        let cfg = Config { dir: base.clone(), file: base.join("Snowmeter.ini"), section: "Snowmeter" };
        fs::write(&cfg.file, "[Snowmeter]\n").unwrap();

        // Case-insensitive order, as on Windows: A.ini is 1, b.ini is 2
        assert_eq!(config_files(&root, "suite/clock"), ["A.ini", "b.ini"]);
        save_active(&cfg, "Suite/Clock", 2).unwrap();
        assert_eq!(fs::read_to_string(&cfg.file).unwrap(), "[Snowmeter]\n\n[Suite\\Clock]\nActive=2\n");
        assert_eq!(active_skins(&cfg, &root), [root.join("Suite/Clock/b.ini")]);
        // 0 (not loaded) and numbers past the last file load nothing
        for n in [0, 3] {
            save_active(&cfg, "Suite\\Clock", n).unwrap();
            assert!(active_skins(&cfg, &root).is_empty());
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn missing_settings_are_written_once() {
        let base = env::temp_dir().join(format!("snowmeter-test-settings-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let cfg = Config { dir: base.clone(), file: base.join("Snowmeter.ini"), section: "Snowmeter" };
        // A skin with only Active and a hand-set Draggable=0
        fs::write(&cfg.file, "[Suite\\Clock]\nActive=1\nDraggable=0\n").unwrap();

        let s = read_skin_settings(&cfg, "Suite/Clock");
        save_missing_settings(&cfg, "Suite/Clock", &s).unwrap();
        let text = fs::read_to_string(&cfg.file).unwrap();
        for line in ["WindowX=0", "WindowY=0", "AlwaysOnTop=0", "SnapEdges=1", "ClickThrough=0", "KeepOnScreen=1"] {
            assert!(text.contains(line), "{line} missing in:\n{text}");
        }
        assert!(text.contains("Draggable=0") && !text.contains("Draggable=1")); // left alone
        // A second time there's nothing to add, so the file is the same
        save_missing_settings(&cfg, "Suite/Clock", &read_skin_settings(&cfg, "Suite/Clock")).unwrap();
        assert_eq!(fs::read_to_string(&cfg.file).unwrap(), text);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn skins_make_a_tree() {
        let skins: Vec<String> = [
            "illustro/Clock/Clock.ini",
            "Suite/Main.ini",
            "Suite/Parts/Bar.ini",
            "Suite/Parts/Deep/Deep.ini",
            "Suite/Alt.ini",
            "Backup/Old/Old.ini",
            "Loose.ini",
            "Clock-x/A.ini",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let tree = tree_from(&skins);
        let names: Vec<&str> = tree.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Clock-x", "illustro", "Suite"], "by name, no backup, no loose file");
        assert!(tree[1].files.is_empty(), "illustro holds only a folder");
        assert_eq!(tree[1].folders[0].files, ["Clock.ini"]);
        let suite = &tree[2];
        assert_eq!(suite.files, ["Alt.ini", "Main.ini"]);
        assert_eq!(suite.folders[0].name, "Parts");
        assert_eq!(suite.folders[0].files, ["Bar.ini"]);
        assert_eq!(suite.folders[0].folders[0].files, ["Deep.ini"]);
    }
    
    #[test]
    fn folders_without_skins_stay_out() {
        let root = env::temp_dir().join(format!("snowmeter-test-tree-{}", std::process::id()));
        for dir in ["Suite/Clock", "Suite/@Resources", "NoSkin", "Empty/Inner"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("Suite/Clock/Clock.ini"), "").unwrap();
        fs::write(root.join("Suite/@Resources/Vars.ini"), "").unwrap();
        fs::write(root.join("NoSkin/NoSkin.txt"), "").unwrap();

        let tree = skin_tree(&root);
        let names: Vec<&str> = tree.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Suite"], "NoSkin has no .ini, Empty has no files at all");
        let below: Vec<&str> = tree[0].folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(below, ["Clock"], "@Resources isn't a skin folder");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn first_load_has_no_settings() {
        let base = env::temp_dir().join(format!("snowmeter-test-first-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let cfg = Config { dir: base.clone(), file: base.join("Snowmeter.ini"), section: "Snowmeter" };
        assert!(!has_settings(&cfg, "Suite\\Clock"));           // no file yet
        fs::write(&cfg.file, "[Suite\\Clock]\n\n[Other]\nActive=1\n").unwrap();
        assert!(!has_settings(&cfg, "Suite\\Clock"));           // a section with no keys
        let defaults = [("WindowX", "200".to_string()), ("Draggable", "0".to_string())];
        save_options(&cfg, "Suite/Clock", &defaults).unwrap();
        assert!(has_settings(&cfg, "Suite/Clock"));
        let s = read_skin_settings(&cfg, "Suite/Clock");
        assert_eq!((s.position.x.as_str(), s.draggable), ("200", false));
        fs::remove_dir_all(&base).unwrap();
    }
}