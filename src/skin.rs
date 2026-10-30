use crate::ini;
use crate::measure::{number_option, Measure, Ranged, Switch};
use crate::meter::{self, Layout, Meter};
use crate::mouse::{MouseActions, MouseEvent};
use crate::{calc, clock, cpu, fonts, formula, memory};
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
    pub background: Option<meter::Fill>, // BackgroundMode=2: SolidColor, SolidColor2, GradientAngle
    pub width: i32,
    pub height: i32,
    pub dynamic_window: bool, // DynamicWindowSize=1
    pub fixed_size: (i32, i32), // SkinWidth/SkinHeight (0 = follow the meter)
    pub drag_margins: [i32; 4], // DragMargins=Left,Top,Right,Bottom (all 0 = drag anywhere)
    pub mouse: MouseActions,  // mouse actions of [Rainmeter], for clicks no meter takes
    // OnRefreshAction, OnUpdateAction and OnCloseAction of [Rainmeter] (none = not set)
    pub on_refresh: Option<String>,
    pub on_update: Option<String>,
    pub on_close: Option<String>,
    pub on_focus: Option<String>,
    pub on_unfocus: Option<String>,
    pub vars: HashMap<String, String>, // the variable table, kept so !SetVariable can change it
    pub file: PathBuf,         // the skin's .ini, the skins folder and the settings folder:
    pub skins_root: PathBuf,   // what load() needs to read the skin again for !Refresh
    pub settings_dir: PathBuf,
    pub config: String, // the config name, like illustro\Clock (#CURRENTCONFIG#)
}

impl Skin {
    // The action a mouse event at (x, y) in the window runs, if any,
    // with the name of the section it belongs to
    pub fn action_at(&self, x: f64, y: f64, event: MouseEvent) -> Option<(&str, &str)> {
        meter::action_at(&self.meters, &self.mouse, &self.measures, x, y, event)
    }

    // Fills in [#Var] and [&Measure] in an action just before it runs, so it sees the latest
    // !SetVariable and the values of that moment. `section` is the action's own section,
    // for [#CURRENTSECTION].
    pub fn expand_action(&self, section: &str, text: &str) -> String {
        let has_sections = text.contains("[&");
        if !has_sections && !text.contains("[#") && !text.contains("[\\") && !text.contains("[$") {
            return text.to_string(); // the usual case: no copy of the variable table
        }
        let vars = section_vars(&self.vars, section);
        if !has_sections {
            return ini::resolve_nested(text, &vars);
        }
        let values = crate::measure::current_values(&self.measures);
        let geom = Layout::geometry_of(&self.meters, &self.measures);
        ini::resolve_nested_with(text, &vars, &|name| {
            crate::sectionvar::lookup_section(name, &values, &geom)
        })
    }

    // The meter called `name`, for bangs like !HideMeter. Names ignore case, as in Rainmeter.
    pub fn meter_mut(&mut self, name: &str) -> Option<&mut Meter> {
        self.meters.iter_mut().find(|m| m.name().eq_ignore_ascii_case(name))
    }

    // The Disabled/Paused state of the measure called `name`, for bangs like
    // !DisableMeasure. Names ignore case, as in Rainmeter.
    pub fn measure_switch(&mut self, name: &str) -> Option<&mut Switch> {
        let m = self.measures.iter_mut().find(|m| m.name().eq_ignore_ascii_case(name))?;
        m.switch_mut()
    }
}

/* Whether a press at (x, y) may drag the skin, as in Skin.cpp OnNcHitTest:
   a negative left/top count from the far edge, and so does a right/bottom
   bigger than the skin. */
pub fn in_drag_area(margins: [i32; 4], (w,h): (i32, i32), x: f64, y:f64) -> bool {
    let [left, top, right, bottom] = margins;
    let x1 = if left < 0 { left + w } else { left };
    let x2 = if w - right > w { w - right - w } else { w - right };
    let y1 = if top < 0 { top + h } else { top };
    let y2 = if h - bottom > h { h - bottom - h } else { h - bottom };
    x >= x1 as f64 && x < x2 as f64 && y >= y1 as f64 && y < y2 as f64
}

/* Window size = bounding box of all meters plus a small margin. A side of
   'fixed' (SkinWidth/SkinHeight) above 0 replace that side , as in Rainmeter. */
pub fn window_size(meters: &[Meter], measures: &[Box<dyn Measure>], fixed: (i32, i32)) -> (i32, i32) {
    let (mut right, mut bottom) = (0.0_f64, 0.0_f64);
    for m in meters {
        let (r, b) = m.bounds(measures);
        right = right.max(r);
        bottom = bottom.max(b);
    }
    let side = |v: f64, fixed: i32| {
        if fixed > 0 {
            fixed.min(MAX_WINDOW_SIDE as i32)
        } else {
            (v + 10.0).ceil().clamp(50.0, MAX_WINDOW_SIDE) as i32
        }
    };
    (side(right, fixed.0), side(bottom, fixed.1))
}

// The variables as one section sees them: everything, plus #CURRENTSECTION#
// set to that section's name. Being a normal variable, it also works inside
// other variables and options inherited through MeterStyle.
fn section_vars(vars: &HashMap<String, String>, name: &str) -> HashMap<String, String> {
    let mut own = vars.clone();
    own.insert("currentsection".to_string(), name.to_string());
    own
}

// Every section by lowercase name, built once so MeterStyle lookups don't scan the file.
// If a name appears twice, the first one wins.
fn section_index(all: &[ini::Section]) -> HashMap<String, &ini::Section> {
    let mut index = HashMap::new();
    for s in all {
        index.entry(s.name.to_lowercase()).or_insert(s);
    }
    index
}

// MeterStyle=Parent1|Parent2: builds the meter's full set of options.
// Later parents beat earlier ones, and the meter's own options beat them all.
fn apply_meter_styles(
    section: &ini::Section,
    index: &HashMap<String, &ini::Section>,
    vars: &HashMap<String, String>,
) -> ini::Section {
    let mut options: HashMap<String, String> = HashMap::new();

    if let Some(list) = section.options.get("meterstyle") {
        let list = ini::resolve(list, vars);
        let mut names = list.split('|').map(str::trim).filter(|n| !n.is_empty());
        // by_ref(): take() uses up only the first MAX_STYLES names and leaves the rest
        // in `names`, so they can be counted below without ever being stored
        for name in names.by_ref().take(MAX_STYLES) {
            let Some(parent) = index.get(&name.to_lowercase()) else {
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
pub fn load(skin_file: &Path, skins_root: &Path, settings_dir: &Path) -> Result<Skin, String> {
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

    // Config names use backslashes as on Windows (illustro\Clock), since skins pass them to bangs
    let config_name = |p: &Path| match p.strip_prefix(skins_root) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().replace('/', "\\"),
        _ => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
    };
    let config = config_name(&base_dir);
    builtin.insert("currentconfig".to_string(), config.clone());
    builtin.insert("rootconfig".to_string(), config_name(&root_config));
    builtin.insert("settingspath".to_string(), dir(settings_dir));
    builtin.insert("addonspath".to_string(), dir(&settings_dir.join("Addons")));
    // The program's folder; Rainmeter keeps plugins in a Plugins folder beside it
    let program_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    builtin.insert("programpath".to_string(), dir(&program_dir));
    builtin.insert("pluginspath".to_string(), dir(&program_dir.join("Plugins")));
    builtin.insert("programdrive".to_string(), "/".to_string()); // Linux has no drive letters
    builtin.insert("crlf".to_string(), "\n".to_string());
    // The user's preferred editor, looked up the way Linux programs do
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|e| !e.is_empty()))
        .unwrap_or_else(|| "xdg-open".to_string());
    builtin.insert("configeditor".to_string(), editor);
    builtin.extend(crate::monitors::screen_vars());

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
        if value.contains("[#") || value.contains("[\\") {
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
        let inner: Box<dyn Measure> = match kind.to_lowercase().as_str() {
            "cpu" => Box::new(cpu::CpuMeasure::new(&section.name)),
            "physicalmemory" => Box::new(memory::MemoryMeasure::new(&section.name)),
            "time" => Box::new(clock::TimeMeasure::from_options(section, &vars)),
            "calc" => Box::new(calc::CalcMeasure::from_options(section, &vars)),
            other => {
                eprintln!(
                    "[{}] unsupported measure type '{}', skipping",
                    section.name, other
                );
                continue;
            }
        };

        // MinValue / MaxValue override the measure's own range. Every measure gets a
        // Ranged, so a later !SetOption can add them.
        let mut ranged = Ranged::wrap(inner);
        ranged.read_range(section, &vars);

        // Disabled= / Paused=: the Switch goes outside, so bangs can reach it (switch_mut)
        let flag = |key: &str| number_option(section, key, &vars).unwrap_or(0.0) != 0.0;
        let switch = Switch::new(Box::new(ranged), flag("disabled"), flag("paused"));
        measures.push(Box::new(switch.with_source(section, &vars)));
    }

    // Like Rainmeter, evaluate every measure once at load, so values exist from the start
    let values = crate::measure::update_all(&mut measures);

    // Fonts must be loaded before the meters, which measure their text when built
    let rm = ini.section("Rainmeter");
    fonts::load_skin_fonts(&root_config.join("@Resources"), &base_dir, |key| {
        rm.and_then(|s| s.options.get(key)).map(|v| ini::resolve(v, &vars))
    });

    // Pass 2: meters, in order. Each is placed as soon as it is built, so [MeterName:X]
    // and relative positions (X=10R) can use the meters before it.
    let mut meters: Vec<Meter> = Vec::new();
    let mut layout = Layout::new();
    let index = section_index(&ini.sections);
    for section in &ini.sections {
        // Only a section that says Meter= itself is a meter (a style section is just a style)
        if !section.options.contains_key("meter") {
            continue;
        }
        let vars = section_vars(&vars, &section.name);
        // MeterStyle=Parent1|Parent2: inherit options before the meter is built
        let styled = apply_meter_styles(section, &index, &vars);
        if let Some(mut m) =
            Meter::from_section(&styled, &vars, &measures, &values, layout.geometry(), &base_dir)
        {
            layout.place(&mut m, &measures);
            meters.push(m);
        }
    }

    // [Rainmeter] section: Update, DynamicWindowSize, BackgroundMode=2 + SolidColor/SolidColor2/GradientAngle
    let get = |key: &str| {
        rm.and_then(|s| s.options.get(key))
            .map(|v| ini::resolve(v, &vars))
    };
    let update_ms: u64 = get("update").and_then(|v| v.parse().ok()).unwrap_or(1000);
    let dynamic_window = get("dynamicwindowsize")
        .and_then(|v| formula::parse_number(&v).ok())
        .unwrap_or(0.0)
        != 0.0;
    // SkinWidth / SkinHeight: whole numbers, formulas allowed; 0 or less = not set
    let int = |key: &str| {
        get(key)
        .and_then(|v| formula::parse_number(&v).ok())
        .map_or(0, |n| n as i32)
    };
    let fixed_size = (int("skinwidth"), int("skinheight"));
    // DragMargins=Left,Top,Right,Bottom: whole numbers, formulas allowed; missing = 0
    let mut drag_margins = [0; 4];
    if let Some(v) = get("dragmargins") {
        for (slot, part) in drag_margins.iter_mut().zip(v.split(',')) {
            *slot = formula::parse_number(part.trim()).map_or(0, |n| n as i32);
        }
    }
    let background = if get("backgroundmode").as_deref() == Some("2") {
        // SolidColor default to gray (as in skin.cpp), SolideColor2 to SolidColor, GradientAngle to 0
        let gray = meter::Color { r: 128.0 / 255.0, g: 128.0 / 255.0, b: 128.0 / 255.0, a: 1.0 };
        let color = get("solidcolor").and_then(|v| meter::parse_color(&v)).unwrap_or(gray);
        let color2 = get("solodcolor2").and_then(|v| meter::parse_color(&v)).unwrap_or(color);
        let angle = get("gradientangle")
            .and_then(|v| formula::parse_number(&v).ok())
            .filter(|a| a.is_finite())
            .unwrap_or(0.0);
        Some(meter::Fill { color, color2, angle})
    } else {
        None
    };
    let mouse = rm
        .map(|s| MouseActions::read(s, &section_vars(&vars, &s.name)))
        .unwrap_or_default();

    //The [Rainmeter] action. Like the mouse action, #Var# is filled in now and
    //[#Var] when they run; an empty one counts as not set.
    let rm_vars = section_vars(&vars, "Rainmeter");
    let action = |key: &str| {
        rm.and_then(|s| s.options.get(key))
            .map(|v| ini::resolve(v, &rm_vars))
            .filter(|a| !a.trim().is_empty())
    };
    let on_refresh = action("onrefreshaction");
    let on_update = action("onupdateaction");
    let on_close = action("oncloseaction");
    let on_focus = action("onfocusaction");
    let on_unfocus = action("onunfocusaction");

    let (width, height) = window_size(&meters, &measures, fixed_size);
    Ok(Skin {
        measures,
        meters,
        update_ms: update_ms.max(50),
        background,
        width,
        height,
        dynamic_window,
        fixed_size,
        drag_margins,
        mouse,
        on_refresh,
        on_update,
        on_close,
        on_focus,
        on_unfocus,
        vars,
        file: skin_file.to_path_buf(),
        skins_root: skins_root.to_path_buf(),
        settings_dir: settings_dir.to_path_buf(),
        config,
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

    #[test]
    fn drag_margins_area() {
        let size = (100, 50);
        assert!(in_drag_area([0, 0, 0, 0], size, 99.0, 49.0)); //default: everywhere
        assert!(!in_drag_area([0, 0, 0, 30], size, 10.0, 25.0)); // bottom 30 px blocked
        assert!(in_drag_area([0, 0, 0, 30], size, 10.0, 19.0));
        assert!(!in_drag_area([-20, 0, 0, 0], size, 70.0, 10.0)); // only the last 20 px
        assert!(in_drag_area([-20, 0, 0, 0], size, 85.0, 10.0,));
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
        let merged = apply_meter_styles(&all[2], &section_index(&all), &no_vars());
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
        let merged = apply_meter_styles(&all[2], &section_index(&all), &no_vars());
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
        let merged = apply_meter_styles(&all[2], &section_index(&all), &no_vars());
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
        let merged = apply_meter_styles(&all[1], &section_index(&all), &vars);
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
            let merged = apply_meter_styles(child, &section_index(&all), &vars);
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
        let merged = apply_meter_styles(all.last().unwrap(), &section_index(&all), &no_vars());
        assert_eq!(get(&merged, "o15"), Some("1"));
        assert_eq!(get(&merged, "o16"), None);
    }
}