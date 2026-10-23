use crate::{paths, textfile};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// Safety limits: skins are untrusted input
const MAX_INCLUDE_DEPTH: usize = 8; // how deeply includes may nest
const MAX_INCLUDES: usize = 64; // total @Include files per skin
const MAX_VAR_DEPTH: usize = 16; // variables inside variables, this many levels deep
const MAX_RESOLVED_LEN: usize = 64 * 1024; // longest text an option may grow to

pub struct Section {
    pub name: String,                     // original capitalization
    pub options: HashMap<String, String>, // keys stored lowercase
}

pub struct Ini {
    pub sections: Vec<Section>, // kept in file order, which matters in Rainmeter
}

// A section as written in a file: options in reading order, @Include lines still present
struct RawSection {
    name: String,
    pairs: Vec<(String, String)>,
}

fn parse_raw(text: &str) -> Vec<RawSection> {
    let mut sections: Vec<RawSection> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue; // blank line or comment
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push(RawSection {
                name: line[1..line.len() - 1].trim().to_string(),
                pairs: Vec::new(),
            });
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some(section) = sections.last_mut() {
                section
                    .pairs
                    .push((key.trim().to_lowercase(), value.trim().to_string()));
            }
        }
    }
    sections
}

fn is_include(key: &str) -> bool {
    key.starts_with("@include") // keys are already lowercase
}

// Later options overwrite earlier ones; @Include lines are not real options
fn from_raw(raw: Vec<RawSection>) -> Ini {
    let sections = raw
        .into_iter()
        .map(|r| {
            let mut options = HashMap::new();
            for (k, v) in r.pairs {
                if !is_include(&k) {
                    options.insert(k, v);
                }
            }
            Section { name: r.name, options }
        })
        .collect();
    Ini { sections }
}

impl Ini {
    // Parses text without following @Include (used for small config files)
    pub fn parse(text: &str) -> Ini {
        from_raw(parse_raw(text))
    }

    // Section names are case-insensitive in Rainmeter
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

// Replaces #Name# with the value of the variable Name (case-insensitive).
// Like in Rainmeter, a value may use other variables in turn (A=#B#).
pub fn resolve(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    resolve_into(&mut out, value, vars, 0);
    truncate_to_cap(&mut out);
    out
}

// Cuts text down to MAX_RESOLVED_LEN bytes
fn truncate_to_cap(s: &mut String) {
    if s.len() > MAX_RESOLVED_LEN {
        // Cut on a character boundary: a UTF-8 character can be several bytes long
        let mut cut = MAX_RESOLVED_LEN;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
}

// Appends `value` to `out` with its variables replaced, following variables inside
// variables up to MAX_VAR_DEPTH deep. Every level writes into the same `out`, and
// stops once it is MAX_RESOLVED_LEN long, so hostile chains stay cheap.
fn resolve_into(out: &mut String, value: &str, vars: &HashMap<String, String>, depth: usize) {
    let mut rest = value;
    while let Some(start) = rest.find('#') {
        if out.len() >= MAX_RESOLVED_LEN {
            return;
        }
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('#') {
            let key = after[..end].to_lowercase();
            if let Some(v) = vars.get(&key) {
                if depth < MAX_VAR_DEPTH {
                    resolve_into(out, v, vars, depth + 1);
                } else {
                    out.push_str(v); // too deep (A=#A#?): keep the text as it is
                }
                rest = &after[end + 1..];
                continue;
            }
        }
        // Not a known variable: keep the '#' as plain text
        out.push('#');
        rest = after;
    }
    out.push_str(rest);
}

const MAX_NESTED_DEPTH: usize = 10; // Rainmeter's maxRecursionDepth
// A '[' is only looked for this many bytes before its ']'. Rainmeter has no limit, but
// then text like [[[[…]]]] takes time that grows with the square of its length.
const MAX_NESTED_SPAN: usize = 1024;

// Replaces new-style variables [#Name] with the variable's value, as Rainmeter's
// ExpandSectionVariables does for the '#' key. Works from the innermost [ ] outward,
// so [#Color[#N]] reads [#N] first. [#*Name*] is an escape: it becomes the text [#Name].
// Unknown names, and other brackets such as [Measure] or [!Bang], stay as written.
pub fn resolve_nested(text: &str, vars: &HashMap<String, String>) -> String {
    resolve_nested_with(text, vars, &|_| None)
}

// Like resolve_nested, and also [&Name], Rainmeter's nested section variable:
// `section` gives the value of "Name" (or "Name:params"), or None if it's unknown.
// Both kinds are done in the same pass, so [#Color[&Index]] and [&Measure[#N]] work.
pub fn resolve_nested_with(
    text: &str,
    vars: &HashMap<String, String>,
    section: &dyn Fn(&str) -> Option<String>,
) -> String {
    let mut s = text.to_string();
    resolve_nested_in(&mut s, vars, section, 0);
    truncate_to_cap(&mut s);
    s
}

fn resolve_nested_in(
    s: &mut String,
    vars: &HashMap<String, String>,
    section: &dyn Fn(&str) -> Option<String>,
    depth: usize,
) {
    let mut from = 0; // where to look for the next ']'
    while let Some(offset) = s[from..].find(']') {
        if s.len() > MAX_RESOLVED_LEN {
            return;
        }
        let end = from + offset;
        let mut next = end + 1; // by default, carry on after this ']'
        let mut start = end;
        let floor = end.saturating_sub(MAX_NESTED_SPAN);
        // The '[' signs before this ']', nearest first. Searched as bytes: `floor` may fall
        // inside a multi-byte character, and '[' is never part of one.
        loop {
            let Some(open) = s.as_bytes()[floor..start].iter().rposition(|&b| b == b'[') else {
                break;
            };
            start = floor + open;
            let inner = &s[start + 1..end]; // the text between [ and ]
            if inner.len() < 2 {
                break; // "[]" or "[#]": nothing to look up
            }
            // The key: '#' for a variable, '&' for a section variable. Else try an earlier [
            let is_var = inner.starts_with('#');
            if !is_var && !inner.starts_with('&') {
                continue;
            }
            let name = &inner[1..];
            if name.len() >= 2 && name.starts_with('*') && name.ends_with('*') {
                // [#*Name*] → [#Name]: drop the two stars (the later one first,
                // so the earlier index is still right)
                s.remove(end - 1);
                s.remove(start + 2);
                next = end - 1; // the ']' moved two places left
                break;
            }
            let found = if is_var { vars.get(&name.to_lowercase()).cloned() } else { section(name) };
            if let Some(mut value) = found {
                if depth < MAX_NESTED_DEPTH {
                    // the value may hold [#…] or [&…] too
                    resolve_nested_in(&mut value, vars, section, depth + 1);
                }
                s.replace_range(start..=end, &value);
                next = start + value.len(); // carry on right after the inserted value
                break;
            }
            // An unknown name: like Rainmeter, try an earlier [
        }
        from = next;
    }
}

// Reads a skin file and everything it @Includes.
//   builtin_vars  built-in variables such as #@#, usable inside @Include paths
//   base_dir      relative include paths are looked up here
//   allowed_root  include files must live inside this folder
pub fn load_with_includes(
    path: &Path,
    builtin_vars: &HashMap<String, String>,
    base_dir: &Path,
    allowed_root: &Path,
) -> Result<Ini, String> {
    let text = textfile::read_text(path)?;
    let raw = parse_raw(&text);

    // Include paths may use variables from the main file's [Variables] section
    let mut vars: HashMap<String, String> = HashMap::new();
    for s in raw
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case("variables"))
    {
        for (k, v) in &s.pairs {
            if !is_include(k) {
                vars.insert(k.clone(), v.clone());
            }
        }
    }
    for (k, v) in builtin_vars {
        vars.insert(k.clone(), v.clone()); // built-ins win
    }

    let main_canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut loader = Loader {
        vars,
        base_dir: base_dir.to_path_buf(),
        allowed_root: allowed_root.to_path_buf(),
        stack: vec![main_canon],
        count: 0,
    };
    let sections = loader.expand(raw);
    Ok(from_raw(sections))
}

struct Loader {
    vars: HashMap<String, String>,
    base_dir: PathBuf,
    allowed_root: PathBuf,
    stack: Vec<PathBuf>, // files currently being loaded, to catch include loops
    count: usize,        // include files loaded so far
}

impl Loader {
    // Loads one include file (and, through expand, anything it includes)
    fn load_file(&mut self, path: &Path) -> Result<Vec<RawSection>, String> {
        let canon = path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve {}: {}", path.display(), e))?;
        if self.stack.contains(&canon) {
            return Err(format!(
                "{} includes itself (include loop), skipping it",
                canon.display()
            ));
        }
        if self.stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(format!(
                "includes are nested more than {} levels deep, skipping {}",
                MAX_INCLUDE_DEPTH,
                canon.display()
            ));
        }
        let text = textfile::read_text(&canon)?;
        self.stack.push(canon);
        let sections = self.expand(parse_raw(&text));
        self.stack.pop();
        Ok(sections)
    }

    // Finds, checks and loads the file named by an @Include value.
    // Problems are reported and the include is skipped, like Rainmeter does.
    fn load_include(&mut self, value: &str) -> Option<Vec<RawSection>> {
        if self.count >= MAX_INCLUDES {
            eprintln!(
                "too many @Include files (limit {}), skipping '{}'",
                MAX_INCLUDES, value
            );
            return None;
        }
        self.count += 1;

        let name = resolve(value, &self.vars);
        let Some(found) = paths::find_file(&self.base_dir, &name) else {
            eprintln!("@Include file '{}' not found", name);
            return None;
        };
        let canon = match found.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("@Include '{}': {}", name, e);
                return None;
            }
        };
        if !canon.starts_with(&self.allowed_root) {
            eprintln!(
                "@Include '{}' is outside the skin's folder, refusing to load it",
                name
            );
            return None;
        }
        match self.load_file(&canon) {
            Ok(sections) => Some(sections),
            Err(e) => {
                eprintln!("@Include: {}", e);
                None
            }
        }
    }

    // Walks the sections of one file in order, splicing in includes as they appear
    fn expand(&mut self, file_sections: Vec<RawSection>) -> Vec<RawSection> {
        // Every section this file defines, so we know which included sections are "new"
        let main_names: HashSet<String> = file_sections
            .iter()
            .map(|s| s.name.to_lowercase())
            .collect();
        let mut out: Vec<RawSection> = Vec::new();
        // Options an include supplied for a section this file defines further down
        let mut pending: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for raw in file_sections {
            let key = raw.name.to_lowercase();
            let idx = match out.iter().position(|s| s.name.to_lowercase() == key) {
                Some(i) => i, // a second [Section] with the same name merges into the first
                None => {
                    out.push(RawSection {
                        name: raw.name.clone(),
                        pairs: pending.remove(&key).unwrap_or_default(),
                    });
                    out.len() - 1
                }
            };

            for (k, v) in raw.pairs {
                if !is_include(&k) {
                    out[idx].pairs.push((k, v));
                    continue;
                }
                let Some(included) = self.load_include(&v) else {
                    continue;
                };
                for inc in included {
                    let ikey = inc.name.to_lowercase();
                    if let Some(i) = out.iter().position(|s| s.name.to_lowercase() == ikey) {
                        // Section already exists: add the options in reading order
                        out[i].pairs.extend(inc.pairs);
                    } else if main_names.contains(&ikey) {
                        // This file defines it further down: the include's options go first,
                        // so the main file's own lines override them
                        pending.entry(ikey).or_default().extend(inc.pairs);
                    } else {
                        // Brand new section: goes right after the calling section
                        out.push(inc);
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn variables_can_use_other_variables() {
        let v = vars(&[("accent", "76,178,255"), ("title", "#Accent#"), ("both", "#TITLE#/#accent#")]);
        assert_eq!(resolve("#Title#", &v), "76,178,255");
        assert_eq!(resolve("[#both#]", &v), "[76,178,255/76,178,255]");
        assert_eq!(resolve("#unknown# and #", &v), "#unknown# and #");
    }

    #[test]
    fn a_variable_using_itself_stops() {
        let v = vars(&[("a", "#A#")]);
        assert_eq!(resolve("#A#", &v), "#A#");
    }

    #[test]
    fn doubling_chains_are_capped() {
        // a0 = "xx", a1 = "#a0##a0#", a2 = "#a1##a1#", ... would double 20 times
        let mut pairs: Vec<(String, String)> = vec![("a0".into(), "xx".into())];
        for i in 1..=20 {
            pairs.push((format!("a{}", i), format!("#a{}##a{}#", i - 1, i - 1)));
        }
        let v: HashMap<String, String> = pairs.into_iter().collect();
        let out = resolve("#a20#", &v);
        assert_eq!(out.len(), MAX_RESOLVED_LEN);
    }

    #[test]
    fn nested_variables_are_replaced() {
        let v = vars(&[("currentsection", "MeterTitle"), ("n", "2"), ("color2", "red"), ("a", "[#B]"), ("b", "hi")]);
        assert_eq!(resolve_nested("[!HideMeter [#CURRENTSECTION]]", &v), "[!HideMeter MeterTitle]");
        assert_eq!(resolve_nested("[#Color[#N]]", &v), "red"); // innermost first
        assert_eq!(resolve_nested("[#A]", &v), "hi"); // the value is expanded too
        assert_eq!(resolve_nested("x[#n]y[#N]z", &v), "x2y2z");
    }

    #[test]
    fn nested_escapes_and_unknowns_stay() {
        let v = vars(&[("a", "1")]);
        assert_eq!(resolve_nested("[#*A*]", &v), "[#A]");
        assert_eq!(resolve_nested("[#*A*]][#A]", &v), "[#A]]1");
        for text in ["[#Nope]", "[MeasureCPU]", "[#]", "[]", "]][[", "[#A"] {
            assert_eq!(resolve_nested(text, &v), text);
        }
    }

    #[test]
    fn nested_variable_using_itself_stops() {
        let v = vars(&[("a", "[#A]")]);
        assert_eq!(resolve_nested("[#A]", &v), "[#A]");
    }

    #[test]
    fn nested_hostile_input_stays_fast() {
        let v = vars(&[("a", "1")]);
        // 60 000 bytes: under MAX_RESOLVED_LEN, so nothing is cut off
        let text = "[".repeat(30_000) + &"]".repeat(30_000);
        assert_eq!(resolve_nested(&text, &v), text);
    }
}
