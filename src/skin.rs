use crate::ini;
use crate::measure::{Measure, Ranged};
use crate::meter::{self, Color, Layout, Meter};
use crate::mouse::{MouseActions, MouseEvent};
use crate::{calc, clock, cpu, formula, memory};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const MAX_WINDOW_SIDE: f64 = 16384.0; // a skin can't ask for an absurdly large window
const MAX_STYLES: usize = 16; // parents a single MeterStyle may list

pub struct Skin {
    pub measures: Vec<Box<dyn Measure>>,
    pub meters: Vec<Meter>,
    pub update_ms: u64,
    pub background: Option<Color>,
    pub width: i32,
    pub height: i32,
    pub dynamic_window: bool, // DynamicWindowSize=1
    pub mouse: MouseActions,  // mouse actions of [Rainmeter], for clicks no meter takes
    pub vars: HashMap<String, String>, // the variable table, kept so !SetVariable can change it
    pub file: PathBuf,       // the skin's .ini and the skins folder: what load() needs
    pub skins_root: PathBuf, // to read the skin again for !Refresh
}

impl Skin {
    // The action a mouse event at (x, y) in the window runs, if any,
    // with the name of the section it belongs to
    pub fn action_at(&self, x: f64, y: f64, event: MouseEvent) -> Option<(&str, &str)> {
        meter::action_at(&self.meters, &self.mouse, &self.measures, x, y, event)
    }

    // Fills in [#Var] in an action just before it runs, so it sees the latest
    // !SetVariable. `section` is the action's own section, for [#CURRENTSECTION].
    pub fn expand_action(&self, section: &str, text: &str) -> String {
        if !text.contains("[#") {
            return text.to_string(); // the usual case: no copy of the variable table
        }
        ini::resolve_nested(text, &section_vars(&self.vars, section))
    }

    // The meter called `name`, for bangs like !HideMeter. Names ignore case, as in Rainmeter.
    pub fn meter_mut(&mut self, name: &str) -> Option<&mut Meter> {
        self.meters.iter_mut().find(|m| m.name().eq_ignore_ascii_case(name))
    }
}

// Window size = bounding box of all meters plus a small margin
pub fn window_size(meters: &[Meter], measures: &[Box<dyn Measure>]) -> (i32, i32) {
    let (mut right, mut bottom) = (0.0_f64, 0.0_f64);
    for m in meters {
        let (r, b) = m.bounds(measures);
        right = right.max(r);
        bottom = bottom.max(b);
    }
    let side = |v: f64| (v + 10.0).ceil().clamp(50.0, MAX_WINDOW_SIDE) as i32;
    (side(right), side(bottom))
}

// Reads a number option such as MinValue. It may be a (formula).
// Bad values are reported and ignored.
fn number_option(
    section: &ini::Section,
    key: &str,
    vars: &HashMap<String, String>,
) -> Option<f64> {
    let raw = section.options.get(key)?;
    let text = ini::resolve(raw, vars);
    match formula::parse_number(&text) {
        Ok(n) => Some(n),
        Err(e) => {
            let hint = if text.contains('[') {
                " (references to other measures need DynamicVariables=1)"
            } else {
                ""
            };
            eprintln!(
                "[{}] cannot read {}={}: {}{}, ignoring it",
                section.name, key, text, e, hint
            );
            None
        }
    }
}

// The variables as one section sees them: everything, plus #CURRENTSECTION#
// set to that section's name. Being a normal variable, it also works inside
// other variables and options inherited through MeterStyle.
fn section_vars(vars: &HashMap<String, String>, name: &str) -> HashMap<String, String> {
    let mut own = vars.clone();
    own.insert("currentsection".to_string(), name.to_string());
    own
}

// MeterStyle=Parent1|Parent2: builds the meter's full set of options.
// Later parents beat earlier ones, and the meter's own options beat them all.
fn apply_meter_styles(
    section: &ini::Section,
    all: &[ini::Section],
    vars: &HashMap<String, String>,
) -> ini::Section {
    let mut options: HashMap<String, String> = HashMap::new();

    if let Some(list) = section.options.get("meterstyle") {
        let list = ini::resolve(list, vars);
        let mut names = list.split('|').map(str::trim).filter(|n| !n.is_empty());
        // by_ref(): take() uses up only the first MAX_STYLES names and leaves the rest
        // in `names`, so they can be counted below without ever being stored
        for name in names.by_ref().take(MAX_STYLES) {
            let Some(parent) = all.iter().find(|s| s.name.eq_ignore_ascii_case(name)) else {
                eprintln!("[{}] MeterStyle '{}' not found, ignoring it", section.name, name);
                continue;
            };
            let reserved = parent.name.eq_ignore_ascii_case("rainmeter")
                || parent.name.eq_ignore_ascii_case("variables");
            if reserved || parent.options.contains_key("measure") {
                eprintln!(
                    "[{}] '{}' can't be used as a MeterStyle, ignoring it",
                    section.name, name
                );
                continue;
            }
            for (k, v) in &parent.options {
                // The meter type and MeterStyle itself are never inherited (no grandparents)
                if k != "meter" && k != "meterstyle" {
                    options.insert(k.clone(), v.clone());
                }
            }
        }
        let extra = names.count();
        if extra > 0 {
            eprintln!(
                "[{}] MeterStyle lists {} styles too many, only the first {} are used",
                section.name, extra, MAX_STYLES
            );
        }
    }

    // The meter's own options win over everything it inherited. Inherited options
    // still hold #CURRENTSECTION# as text; it becomes the child's name when the
    // meter resolves its options with its own section_vars.
    for (k, v) in &section.options {
        options.insert(k.clone(), v.clone());
    }

    ini::Section { name: section.name.clone(), options }
}

// Both paths must be absolute (canonical), so the folder math below is reliable
pub fn load(skin_file: &Path, skins_root: &Path) -> Result<Skin, String> {
    // Folder of this .ini; images and other files are looked up relative to it
    let base_dir = skin_file.parent().unwrap_or(Path::new("/")).to_path_buf();

    // The "root config" is the top-level folder under Skins (Rainmeter's ROOTCONFIGPATH).
    // A skin opened from outside the skins folder uses its own folder instead.
    let root_config = match skin_file.strip_prefix(skins_root) {
        Ok(rel) => {
            let parts: Vec<_> = rel.components().collect();
            if parts.len() >= 2 {
                skins_root.join(parts[0])
            } else {
                base_dir.clone()
            }
        }
        Err(_) => base_dir.clone(),
    };

    // Built-in variables (keys are lowercase, like everything in the variable table)
    let dir = |p: &Path| format!("{}/", p.display());
    let mut builtin: HashMap<String, String> = HashMap::new();
    builtin.insert("@".to_string(), format!("{}/@Resources/", root_config.display()));
    builtin.insert("currentpath".to_string(), dir(&base_dir));
    builtin.insert("rootconfigpath".to_string(), dir(&root_config));
    builtin.insert("skinspath".to_string(), dir(skins_root));
    builtin.insert(
        "currentfile".to_string(),
        skin_file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    );

    // Read the skin and everything it @Includes (include files must live in the root config)
    let ini = ini::load_with_includes(skin_file, &builtin, &base_dir, &root_config)?;

    // [Variables], now including anything that came from include files, with built-ins on top
    let mut vars: HashMap<String, String> = ini
        .section("Variables")
        .map(|s| s.options.clone())
        .unwrap_or_default();
    vars.extend(builtin);

    // [#Name] inside [Variables] values is filled in now, as Rainmeter does when it reads
    // that section. Plain #Name# stays, and is still followed later wherever it's used.
    let snapshot = vars.clone();
    for value in vars.values_mut() {
        if value.contains("[#") {
            *value = ini::resolve_nested(value, &snapshot);
        }
    }

    // Pass 1: measures
    let mut measures: Vec<Box<dyn Measure>> = Vec::new();
    for section in &ini.sections {
        let Some(kind) = section.options.get("measure") else {
            continue;
        };
        let vars = section_vars(&vars, &section.name);
        let kind = ini::resolve(kind, &vars);
        let made: Box<dyn Measure> = match kind.to_lowercase().as_str() {
            "cpu" => Box::new(cpu::CpuMeasure::new(&section.name)),
            "physicalmemory" => Box::new(memory::MemoryMeasure::new(&section.name)),
            "time" => {
                let format = section
                    .options
                    .get("format")
                    .map(|v| ini::resolve(v, &vars))
                    .unwrap_or_else(|| "%H:%M:%S".to_string());
                let tz = section
                    .options
                    .get("timezone")
                    .map(|v| ini::resolve(v, &vars))
                    .and_then(|v| v.trim().parse::<f64>().ok());
                Box::new(clock::TimeMeasure::new(&section.name, &format, tz))
            }
            "calc" => {
                let formula = section
                    .options
                    .get("formula")
                    .map(|v| ini::resolve(v, &vars))
                    .unwrap_or_else(|| "0".to_string());
                let low = number_option(section, "lowbound", &vars).unwrap_or(0.0);
                let high = number_option(section, "highbound", &vars).unwrap_or(100.0);
                let update_random =
                    number_option(section, "updaterandom", &vars).unwrap_or(0.0) != 0.0;
                Box::new(calc::CalcMeasure::new(
                    &section.name,
                    &formula,
                    low,
                    high,
                    update_random,
                ))
            }
            other => {
                eprintln!(
                    "[{}] unsupported measure type '{}', skipping",
                    section.name, other
                );
                continue;
            }
        };

        // MinValue / MaxValue override the measure's own range. With DynamicVariables=1
        // their text is kept and read again on every update; otherwise they are fixed numbers.
        let dynamic = number_option(section, "dynamicvariables", &vars).unwrap_or(0.0) != 0.0;
        if dynamic {
            let source = |key: &str| {
                section.options.get(key).map(|v| ini::resolve_nested(&ini::resolve(v, &vars), &vars))
            };
            let (min_src, max_src) = (source("minvalue"), source("maxvalue"));
            if min_src.is_some() || max_src.is_some() {
                measures.push(Box::new(Ranged::dynamic(made, min_src, max_src)));
            } else {
                measures.push(made);
            }
        } else {
            let min = number_option(section, "minvalue", &vars);
            let max = number_option(section, "maxvalue", &vars);
            if min.is_some() || max.is_some() {
                measures.push(Box::new(Ranged::new(made, min, max)));
            } else {
                measures.push(made);
            }
        }
    }

    // Like Rainmeter, evaluate every measure once at load, so values exist from the start
    let values = crate::measure::update_all(&mut measures);

    // Pass 2: meters, in order. Each is placed as soon as it is built, so [MeterName:X]
    // and relative positions (X=10R) can use the meters before it.
    let mut meters: Vec<Meter> = Vec::new();
    let mut layout = Layout::new();
    for section in &ini.sections {
        // Only a section that says Meter= itself is a meter (a style section is just a style)
        if !section.options.contains_key("meter") {
            continue;
        }
        let vars = section_vars(&vars, &section.name);
        // MeterStyle=Parent1|Parent2: inherit options before the meter is built
        let styled = apply_meter_styles(section, &ini.sections, &vars);
        if let Some(mut m) =
            Meter::from_section(&styled, &vars, &measures, &values, layout.geometry(), &base_dir)
        {
            layout.place(&mut m, &measures);
            meters.push(m);
        }
    }

    // [Rainmeter] section: Update, DynamicWindowSize, BackgroundMode=2 + SolidColor
    let rm = ini.section("Rainmeter");
    let get = |key: &str| {
        rm.and_then(|s| s.options.get(key))
            .map(|v| ini::resolve(v, &vars))
    };
    let update_ms: u64 = get("update").and_then(|v| v.parse().ok()).unwrap_or(1000);
    let dynamic_window = get("dynamicwindowsize")
        .and_then(|v| formula::parse_number(&v).ok())
        .unwrap_or(0.0)
        != 0.0;
    let background = if get("backgroundmode").as_deref() == Some("2") {
        get("solidcolor").and_then(|v| meter::parse_color(&v))
    } else {
        None
    };
    let mouse = rm
        .map(|s| MouseActions::read(s, &section_vars(&vars, &s.name)))
        .unwrap_or_default();

    let (width, height) = window_size(&meters, &measures);
    Ok(Skin {
        measures,
        meters,
        update_ms: update_ms.max(50),
        background,
        width,
        height,
        dynamic_window,
        mouse,
        vars,
        file: skin_file.to_path_buf(),
        skins_root: skins_root.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(name: &str, options: &[(&str, &str)]) -> ini::Section {
        ini::Section {
            name: name.to_string(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn get<'a>(s: &'a ini::Section, key: &str) -> Option<&'a str> {
        s.options.get(key).map(|v| v.as_str())
    }

    fn no_vars() -> HashMap<String, String> {
        HashMap::new()
    }

    // The example from Rainmeter's own MeterStyle page
    #[test]
    fn child_beats_later_parent_beats_earlier_parent() {
        let all = vec![
            section(
                "MyFirstParent",
                &[
                    ("meter", "String"),
                    ("text", "This is a parent!"),
                    ("x", "10"),
                    ("fontcolor", "255,0,0"),
                    ("stringstyle", "Bold"),
                ],
            ),
            section("MySecondParent", &[("stringstyle", "Italic"), ("fontcolor", "0,255,0")]),
            section(
                "MyChild",
                &[
                    ("meter", "String"),
                    ("meterstyle", "MyFirstParent | MySecondParent"),
                    ("x", "0R"),
                    ("fontcolor", "62,189,255"),
                    ("text", "This is a child!"),
                ],
            ),
        ];
        let merged = apply_meter_styles(&all[2], &all, &no_vars());
        assert_eq!(get(&merged, "stringstyle"), Some("Italic"));
        assert_eq!(get(&merged, "fontcolor"), Some("62,189,255"));
        assert_eq!(get(&merged, "x"), Some("0R"));
        assert_eq!(get(&merged, "text"), Some("This is a child!"));
        assert_eq!(get(&merged, "meter"), Some("String"));
        assert_eq!(merged.name, "MyChild");
    }

    #[test]
    fn there_are_no_grandparents() {
        let all = vec![
            section("Grand", &[("fontsize", "20")]),
            section("Parent", &[("meterstyle", "Grand"), ("fontcolor", "1,2,3")]),
            section("Child", &[("meter", "String"), ("meterstyle", "Parent")]),
        ];
        let merged = apply_meter_styles(&all[2], &all, &no_vars());
        assert_eq!(get(&merged, "fontcolor"), Some("1,2,3"));
        assert_eq!(get(&merged, "fontsize"), None);
    }

    #[test]
    fn odd_parents_are_ignored() {
        let all = vec![
            section("Variables", &[("secret", "1")]),
            section("SomeMeasure", &[("measure", "CPU"), ("format", "x")]),
            section(
                "Child",
                &[("meter", "Bar"), ("meterstyle", "Variables|SomeMeasure|Nope|"), ("w", "5")],
            ),
        ];
        let merged = apply_meter_styles(&all[2], &all, &no_vars());
        assert_eq!(get(&merged, "secret"), None);
        assert_eq!(get(&merged, "format"), None);
        assert_eq!(get(&merged, "w"), Some("5"));
    }

    #[test]
    fn names_ignore_case_and_may_use_variables() {
        let all = vec![
            section("StyleA", &[("fontsize", "12")]),
            section("Child", &[("meter", "String"), ("meterstyle", "#Base#")]),
        ];
        let mut vars = no_vars();
        vars.insert("base".to_string(), "stylea".to_string());
        let merged = apply_meter_styles(&all[1], &all, &vars);
        assert_eq!(get(&merged, "fontsize"), Some("12"));
    }

    #[test]
    fn currentsection_is_the_childs_name() {
        let all = vec![
            section("StyleNote", &[("text", "Section: #CurrentSection#")]),
            section("NoteA", &[("meter", "String"), ("meterstyle", "StyleNote")]),
            section(
                "NoteB",
                &[
                    ("meter", "String"),
                    ("meterstyle", "StyleNote"),
                    ("fontcolor", "#CURRENTSECTION#"),
                ],
            ),
        ];
        // Merge the styles, then resolve with the meter's own variables, as load() does
        let resolved = |child: &ini::Section, key: &str| {
            let vars = section_vars(&no_vars(), &child.name);
            let merged = apply_meter_styles(child, &all, &vars);
            ini::resolve(get(&merged, key).unwrap(), &vars)
        };
        assert_eq!(resolved(&all[1], "text"), "Section: NoteA");
        assert_eq!(resolved(&all[2], "text"), "Section: NoteB");
        assert_eq!(resolved(&all[2], "fontcolor"), "NoteB");
    }

    #[test]
    fn currentsection_works_inside_other_variables() {
        let mut vars = no_vars();
        vars.insert("label".to_string(), "I am #CURRENTSECTION#".to_string());
        let vars = section_vars(&vars, "MeterA");
        assert_eq!(ini::resolve("#Label#!", &vars), "I am MeterA!");
    }

    #[test]
    fn too_many_parents_are_cut_off() {
        let mut all: Vec<ini::Section> = (0..20)
            .map(|i| section(&format!("S{}", i), &[(format!("o{}", i).as_str(), "1")]))
            .collect();
        let list: Vec<String> = (0..20).map(|i| format!("S{}", i)).collect();
        all.push(section("Child", &[("meter", "String"), ("meterstyle", &list.join("|"))]));
        let merged = apply_meter_styles(all.last().unwrap(), &all, &no_vars());
        assert_eq!(get(&merged, "o15"), Some("1"));
        assert_eq!(get(&merged, "o16"), None);
    }
}