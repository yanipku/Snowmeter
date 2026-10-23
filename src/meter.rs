use crate::formula;
use crate::ini::{self, Section};
use crate::measure::{Measure, Values};
use crate::mouse::{MouseActions, MouseEvent};
use crate::paths;
use crate::sectionvar::{self, Geometry, Rect};
use gtk4::cairo;
use gtk4::gdk::prelude::GdkCairoContextExt;
use gtk4::gdk_pixbuf::{InterpType, Pixbuf};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
const MAX_IMAGE_SIDE: i32 = 8192;
const MAX_GRAPH_SIDE: f64 = 4096.0; // caps graph memory and drawing work
const MAX_SERIES: usize = 5; // MeasureName .. MeasureName5
const MAX_TEXT_CHARS: usize = 4096; // longer text is cut, so measuring and drawing stay cheap
const MAX_PADDING: f64 = 4096.0; // per side

// Padding: empty space around a meter's content, inside its SolidColor background
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Padding {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

// Accepts "255,255,255", "255,255,255,128", "RRGGBB" or "RRGGBBAA"
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.contains(',') {
        let p: Vec<f64> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        if p.len() == 3 || p.len() == 4 {
            let a = if p.len() == 4 { p[3] } else { 255.0 };
            return Some(Color { r: p[0] / 255.0, g: p[1] / 255.0, b: p[2] / 255.0, a: a / 255.0 });
        }
        None
    } else if s.len() == 6 || s.len() == 8 {
        let v = u32::from_str_radix(s, 16).ok()?;
        if s.len() == 6 {
            Some(Color {
                r: ((v >> 16) & 0xFF) as f64 / 255.0,
                g: ((v >> 8) & 0xFF) as f64 / 255.0,
                b: (v & 0xFF) as f64 / 255.0,
                a: 1.0,
            })
        } else {
            Some(Color {
                r: ((v >> 24) & 0xFF) as f64 / 255.0,
                g: ((v >> 16) & 0xFF) as f64 / 255.0,
                b: ((v >> 8) & 0xFF) as f64 / 255.0,
                a: (v & 0xFF) as f64 / 255.0,
            })
        }
    } else {
        None
    }
}

pub fn set_color(cr: &cairo::Context, c: &Color) {
    cr.set_source_rgba(c.r, c.g, c.b, c.a);
}

// Reads an option from a section and resolves #Variables# in it
fn opt(section: &Section, key: &str, vars: &HashMap<String, String>) -> Option<String> {
    section.options.get(key).map(|v| ini::resolve(v, vars))
}

// None if the option is missing or unreadable (unreadable ones are reported).
// A plain number or a (formula) is accepted, and the result is always finite.
fn num_opt(section: &Section, key: &str, vars: &HashMap<String, String>) -> Option<f64> {
    let text = opt(section, key, vars)?;
    match formula::parse_number(&text) {
        Ok(n) => Some(n),
        Err(e) => {
            eprintln!("[{}] cannot read {}={}: {}", section.name, key, text, e);
            None
        }
    }
}

fn num(section: &Section, key: &str, vars: &HashMap<String, String>, default: f64) -> f64 {
    num_opt(section, key, vars).unwrap_or(default)
}

fn color_opt(section: &Section, key: &str, vars: &HashMap<String, String>) -> Option<Color> {
    opt(section, key, vars).and_then(|v| parse_color(&v))
}

fn find_index(measures: &[Box<dyn Measure>], name: &str) -> Option<usize> {
    measures.iter().position(|m| m.name().eq_ignore_ascii_case(name))
}

// StringCase. Rainmeter uses Windows' LCMapString, and these rules copy what it does
#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    None,
    Upper,
    Lower,
    Proper,
}

// None if the text isn't a valid case
fn parse_case(text: &str) -> Option<Case> {
    match text.trim().to_ascii_lowercase().as_str() {
        "none" => Some(Case::None),
        "upper" => Some(Case::Upper),
        "lower" => Some(Case::Lower),
        "proper" => Some(Case::Proper),
        _ => None,
    }
}

// One character in upper case. Like Windows, a character that would become
// several (ß -> SS) is kept as it is, so the text never changes length.
fn upper(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

// One character in lower case, kept as it is when it would become several (İ)
fn lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

// Proper case: the first letter of each word up, the rest down. Words are runs of
// letters and digits, with apostrophes inside them (it's, rock’n). A word with no
// lower-case letter is taken as an acronym (CPU) and left alone.
// Not covered: the four letters with a separate title form (ǆ gives Ǆ, not ǅ).
fn proper_case(text: &str) -> String {
    let in_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !in_word(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let end = (i..chars.len()).find(|&j| !in_word(chars[j])).unwrap_or(chars.len());
        let word = &chars[i..end];
        if word.iter().any(|c| c.is_lowercase()) {
            let mut first_letter = true;
            for &c in word {
                if first_letter && c.is_alphabetic() {
                    out.push(upper(c));
                    first_letter = false;
                } else {
                    out.push(lower(c));
                }
            }
        } else {
            out.extend(word);
        }
        i = end;
    }
    out
}

fn apply_case(text: &str, case: Case) -> String {
    match case {
        Case::None => text.to_string(),
        Case::Upper => text.chars().map(upper).collect(),
        Case::Lower => text.chars().map(lower).collect(),
        Case::Proper => proper_case(text),
    }
}

// Prefix + template + Postfix, with %1, %2, ... in the template replaced by the text of
// the matching measure, then StringCase. As in Rainmeter, Prefix and Postfix are kept as written.
fn fill_template(
    prefix: &str,
    template: &str,
    postfix: &str,
    case: Case,
    idx: &[Option<usize>],
    measures: &[Box<dyn Measure>],
) -> String {
    let mut body = template.to_string();
    for (n, slot) in idx.iter().enumerate() {
        let value = match slot {
            Some(i) => measures[*i].text(),
            None => String::new(),
        };
        body = body.replace(&format!("%{}", n + 1), &value);
    }
    let mut text = apply_case(&format!("{prefix}{body}{postfix}"), case);
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS).collect();
    }
    text
}

// ---------- text measuring ----------

thread_local! {
    // A 1x1 drawing surface that exists only to measure text with
    static SCRATCH: Option<cairo::Context> = make_scratch();
}

fn make_scratch() -> Option<cairo::Context> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?;
    cairo::Context::new(&surface).ok()
}

// (width, height, ascent) of one line of text in pixels
fn measure_text(
    face: &str,
    slant: cairo::FontSlant,
    weight: cairo::FontWeight,
    size: f64,
    text: &str,
) -> (f64, f64, f64) {
    // Used if measuring isn't possible: the old rough estimate
    let guess = (text.chars().count() as f64 * size * 0.6, size * 1.3, size * 0.85);
    SCRATCH.with(|scratch| {
        let Some(cr) = scratch else { return guess };
        cr.select_font_face(face, slant, weight);
        cr.set_font_size(size);
        let width = cr.text_extents(text).map(|e| e.x_advance()).unwrap_or(guess.0);
        match cr.font_extents() {
            Ok(fe) => (width, fe.ascent() + fe.descent(), fe.ascent()),
            Err(_) => (width, guess.1, guess.2),
        }
    })
}

fn load_image(name: &str, base_dir: &Path, w: Option<f64>, h: Option<f64>) -> Option<Pixbuf> {
    if name.trim().is_empty() {
        return None;
    }
    let path = paths::find_file(base_dir, name)?;

    // Check the size before decoding, so a hostile file can't make us allocate gigabytes
    let (_, iw, ih) = Pixbuf::file_info(&path)?;
    if iw > MAX_IMAGE_SIDE || ih > MAX_IMAGE_SIDE {
        eprintln!("image {} is too large ({}x{}), skipping", path.display(), iw, ih);
        return None;
    }

    let pixbuf = Pixbuf::from_file(&path).ok()?;
    let (ow, oh) = (pixbuf.width() as f64, pixbuf.height() as f64);

    // Like Rainmeter: only W or only H keeps the aspect ratio, both stretches
    let (nw, nh) = match (w, h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, oh * w / ow),
        (None, Some(h)) => (ow * h / oh, h),
        (None, None) => return Some(pixbuf),
    };
    let side = |v: f64| v.round().clamp(1.0, MAX_IMAGE_SIDE as f64) as i32;
    pixbuf.scale_simple(side(nw), side(nh), InterpType::Bilinear)
}

// ---------- positions ----------

// X or Y as written in a skin
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pos {
    Abs(f64),
    // 10r = from the previous meter's top/left edge, 10R = from its bottom/right edge
    Rel { offset: f64, far: bool },
}

fn parse_pos(text: &str) -> Result<Pos, String> {
    let t = text.trim();
    let (number, relative) = match t.chars().last() {
        Some('r') => (&t[..t.len() - 1], Some(false)),
        Some('R') => (&t[..t.len() - 1], Some(true)),
        _ => (t, None),
    };
    // "Y=R" is a shorthand for "Y=0R"
    let n = if number.trim().is_empty() && relative.is_some() {
        0.0
    } else {
        formula::parse_number(number)?
    };
    Ok(match relative {
        Some(far) => Pos::Rel { offset: n, far },
        None => Pos::Abs(n),
    })
}

// StringAlign as (horizontal, vertical) fractions of the meter's size:
// 0.0 = left/top, 0.5 = center, 1.0 = right/bottom. "RightCenter" -> (1.0, 0.5).
// None if the text isn't a valid alignment.
fn parse_align(text: &str) -> Option<(f64, f64)> {
    let t = text.trim().to_ascii_lowercase();
    let (h, rest) = if let Some(rest) = t.strip_prefix("left") {
        (0.0, rest)
    } else if let Some(rest) = t.strip_prefix("center") {
        (0.5, rest)
    } else if let Some(rest) = t.strip_prefix("right") {
        (1.0, rest)
    } else {
        return None;
    };
    let v = match rest {
        "" | "top" => 0.0,
        "center" => 0.5,
        "bottom" => 1.0,
        _ => return None,
    };
    Some((h, v))
}

// Padding=Left,Top,Right,Bottom. Missing values are 0; each may be a (formula).
fn parse_padding(text: &str) -> Result<Padding, String> {
    let mut p = [0.0; 4];
    for (slot, part) in p.iter_mut().zip(text.split(',')) {
        if !part.trim().is_empty() {
            *slot = formula::parse_number(part)?.clamp(0.0, MAX_PADDING);
        }
    }
    Ok(Padding { left: p[0], top: p[1], right: p[2], bottom: p[3] })
}

fn pos_opt(section: &Section, key: &str, vars: &HashMap<String, String>) -> Pos {
    let Some(text) = opt(section, key, vars) else {
        return Pos::Abs(0.0);
    };
    match parse_pos(&text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[{}] cannot read {}={}: {}", section.name, key, text, e);
            Pos::Abs(0.0)
        }
    }
}

// The history and layout shared by Line and Histogram meters
struct Graph {
    history: Vec<VecDeque<f64>>, // one series per measure, oldest sample first
    capacity: usize,             // samples kept = graph width in pixels
    w: f64,
    h: f64,
    autoscale: bool,
    start_left: bool, // GraphStart=Left (newest sample at the left edge)
    flip: bool,       // Flip=1 (graph grows downward)
}

impl Graph {
    fn new(section: &Section, vars: &HashMap<String, String>, series: usize) -> Graph {
        let w = num(section, "w", vars, 1.0).clamp(1.0, MAX_GRAPH_SIDE);
        let h = num(section, "h", vars, 1.0).clamp(1.0, MAX_GRAPH_SIDE);

        if let Some(o) = opt(section, "graphorientation", vars) {
            if o.eq_ignore_ascii_case("horizontal") {
                eprintln!(
                    "[{}] GraphOrientation=Horizontal is not supported yet, drawing it Vertical",
                    section.name
                );
            }
        }

        Graph {
            history: vec![VecDeque::new(); series],
            capacity: (w as usize).max(1),
            w,
            h,
            autoscale: num(section, "autoscale", vars, 0.0) != 0.0,
            start_left: opt(section, "graphstart", vars)
                .map(|v| v.eq_ignore_ascii_case("left"))
                .unwrap_or(false),
            flip: num(section, "flip", vars, 0.0) != 0.0,
        }
    }

    // The value that fills the full height: the biggest sample if AutoScale=1, else 1.0.
    // History holds each measure's value as a 0.0..1.0 fraction of its MinValue..MaxValue range.
    fn max(&self) -> f64 {
        if !self.autoscale {
            return 1.0;
        }
        let biggest = self
            .history
            .iter()
            .flat_map(|series| series.iter().copied())
            .fold(0.0_f64, f64::max);
        if biggest > 0.0 { biggest } else { 1.0 }
    }

    // Value -> 0.0..1.0 of the graph height (Scale is ignored when AutoScale=1)
    fn fraction(&self, value: f64, scale: f64, max: f64) -> f64 {
        let scale = if self.autoscale { 1.0 } else { scale };
        (value * scale / max).clamp(0.0, 1.0)
    }

    // Pixel column of a sample, counting from the newest one
    fn col(&self, from_newest: usize) -> f64 {
        if self.start_left {
            from_newest as f64
        } else {
            (self.capacity - 1 - from_newest) as f64
        }
    }

    // Fraction of the height -> y inside the graph (0 at the top)
    fn y_of(&self, frac: f64) -> f64 {
        if self.flip { frac * self.h } else { self.h - frac * self.h }
    }
}

// A rebuilt graph keeps the history its previous version had collected
fn carry_history(graph: &mut Graph, prev: Option<&Meter>) {
    let old = match prev {
        Some(Meter {
            kind: Kind::Line { graph: old, .. } | Kind::Histogram { graph: old, .. },
            ..
        }) => old,
        _ => return,
    };
    if old.history.len() == graph.history.len() {
        graph.history = old.history.clone();
        for series in graph.history.iter_mut() {
            while series.len() > graph.capacity {
                series.pop_front();
            }
        }
    }
}

enum Kind {
    Text {
        prefix: String,
        template: String,
        postfix: String,
        case: Case,
        size: f64, // in pixels
        color: Color,
        face: String,
        slant: cairo::FontSlant,
        weight: cairo::FontWeight,
        w: Option<f64>,
        h: Option<f64>,
        align: (f64, f64), // StringAlign, see parse_align
    },
    Bar {
        w: f64,
        h: f64,
        color: Color,
        vertical: bool,
    },
    Image {
        pixbuf: Option<Pixbuf>, // None if the file couldn't be loaded
        alpha: f64,             // 0.0 to 1.0
        key: String,            // file + size: an unchanged key means "don't decode it again"
    },
    Line {
        graph: Graph,
        colors: Vec<Color>,        // one per line
        width: f64,
        scales: Vec<f64>,          // one per line
        grid_color: Option<Color>, // Some = draw horizontal marker lines
    },
    Histogram {
        graph: Graph,
        primary: Color,
        secondary: Color,
        both: Color,
    },
}

// What a meter with DynamicVariables=1 needs to rebuild itself on every update
#[derive(Clone)] // Rc::make_mut copies it when a bang changes a shared one
struct DynSource {
    name: String,
    options: HashMap<String, String>, // as written in the skin, [Name] references still in place
    vars: HashMap<String, String>,
    base_dir: PathBuf,
}

pub struct Meter {
    name: String,
    x_pos: Pos, // X and Y as written
    y_pos: Pos,
    x: f64, // where the meter really is (worked out by Layout)
    y: f64,
    solid: Option<Color>,            // SolidColor: background of the whole meter, padding included
    padding: Padding,
    hidden: bool,                    // Hidden=1
    measure_idx: Vec<Option<usize>>, // MeasureName, MeasureName2, ... -> index into the measure list
    kind: Kind,
    dynamic: Option<Rc<DynSource>>, // Some when DynamicVariables=1
    mouse: MouseActions,            // LeftMouseUpAction and the like
}

impl Meter {
    // Returns None for sections that aren't meters (or aren't supported yet).
    // `geom` is where the meters before this one are, for [MeterName:X] references.
    pub fn from_section(
        section: &Section,
        vars: &HashMap<String, String>,
        measures: &[Box<dyn Measure>],
        values: &Values,
        geom: &Geometry,
        base_dir: &Path,
    ) -> Option<Meter> {
        Meter::build(section, vars, measures, values, geom, base_dir, None)
    }

    // `prev` is the earlier version of this same meter when rebuilding for DynamicVariables
    fn build(
        raw: &Section,
        vars: &HashMap<String, String>,
        measures: &[Box<dyn Measure>],
        values: &Values,
        geom: &Geometry,
        base_dir: &Path,
        prev: Option<&Meter>,
    ) -> Option<Meter> {
        // The type of a meter can't be dynamic, so it is read before anything is expanded
        let kind_name = opt(raw, "meter", vars)?.to_lowercase();

        // Section variables like [MeasureCPU:1] and [Other:XW] become the current values
        let expanded = Section {
            name: raw.name.clone(),
            options: sectionvar::expand_options(&raw.options, vars, values, geom),
        };
        let section = &expanded;

        // MeasureName, MeasureName2, MeasureName3, ... (up to 5)
        let mut measure_idx = Vec::new();
        for n in 1..=MAX_SERIES {
            let key = if n == 1 { "measurename".to_string() } else { format!("measurename{}", n) };
            match opt(section, &key, vars) {
                Some(name) => {
                    let found = find_index(measures, &name);
                    if found.is_none() {
                        eprintln!("[{}] measure '{}' not found (or not supported)", section.name, name);
                    }
                    measure_idx.push(found);
                }
                None => break,
            }
        }

        let x_pos = pos_opt(section, "x", vars);
        let y_pos = pos_opt(section, "y", vars);
        let solid = color_opt(section, "solidcolor", vars);
        let padding = match opt(section, "padding", vars) {
            None => Padding::default(),
            Some(text) => parse_padding(&text).unwrap_or_else(|e| {
                eprintln!("[{}] cannot read Padding={}: {}", section.name, text, e);
                Padding::default()
            }),
        };

        let kind = match kind_name.as_str() {
            "string" => {
                let style = opt(section, "stringstyle", vars).unwrap_or_default().to_lowercase();
                let (slant, weight) = match style.as_str() {
                    "bold" => (cairo::FontSlant::Normal, cairo::FontWeight::Bold),
                    "italic" => (cairo::FontSlant::Italic, cairo::FontWeight::Normal),
                    "bolditalic" => (cairo::FontSlant::Italic, cairo::FontWeight::Bold),
                    _ => (cairo::FontSlant::Normal, cairo::FontWeight::Normal),
                };
                let align = match opt(section, "stringalign", vars) {
                    None => (0.0, 0.0),
                    Some(text) => parse_align(&text).unwrap_or_else(|| {
                        eprintln!("[{}] unknown StringAlign={}, using Left", section.name, text);
                        (0.0, 0.0)
                    }),
                };
                let case = match opt(section, "stringcase", vars) {
                    None => Case::None,
                    Some(text) => parse_case(&text).unwrap_or_else(|| {
                        eprintln!("[{}] unknown StringCase={}, using None", section.name, text);
                        Case::None
                    }),
                };
                Kind::Text {
                    prefix: opt(section, "prefix", vars).unwrap_or_default(),
                    // No Text: show the first measure's value, or nothing if there is no measure
                    template: opt(section, "text", vars).unwrap_or_else(|| {
                        if measure_idx.is_empty() { String::new() } else { "%1".to_string() }
                    }),
                    postfix: opt(section, "postfix", vars).unwrap_or_default(),
                    case,
                    // points -> pixels, within sane limits
                    size: num(section, "fontsize", vars, 10.0).clamp(1.0, 1000.0) * 4.0 / 3.0,
                    color: color_opt(section, "fontcolor", vars).unwrap_or(WHITE),
                    face: opt(section, "fontface", vars).unwrap_or_else(|| "Sans".to_string()),
                    slant,
                    weight,
                    w: num_opt(section, "w", vars),
                    h: num_opt(section, "h", vars),
                    align,
                }
            }
            "bar" => Kind::Bar {
                w: num(section, "w", vars, 1.0),
                h: num(section, "h", vars, 1.0),
                color: color_opt(section, "barcolor", vars)
                    .unwrap_or(Color { r: 0.0, g: 128.0 / 255.0, b: 0.0, a: 1.0 }),
                // Rainmeter's default is Vertical; only "Horizontal" switches it
                vertical: !opt(section, "barorientation", vars)
                    .map(|v| v.eq_ignore_ascii_case("horizontal"))
                    .unwrap_or(false),
            },
            "image" => {
                let name = opt(section, "imagename", vars).unwrap_or_default();
                let dir = opt(section, "imagepath", vars).unwrap_or_default();
                let full = format!("{}{}", dir, name);
                let w = num_opt(section, "w", vars);
                let h = num_opt(section, "h", vars);
                let key = format!("{}|{:?}|{:?}", full, w, h);
                let pixbuf = match prev {
                    // Same file and size as last time: reuse what was already decoded
                    Some(Meter { kind: Kind::Image { pixbuf, key: old_key, .. }, .. })
                        if *old_key == key =>
                    {
                        pixbuf.clone()
                    }
                    _ => {
                        let loaded = load_image(&full, base_dir, w, h);
                        if loaded.is_none() {
                            eprintln!("[{}] cannot load image '{}'", section.name, full);
                        }
                        loaded
                    }
                };
                Kind::Image {
                    pixbuf,
                    alpha: (num(section, "imagealpha", vars, 255.0) / 255.0).clamp(0.0, 1.0),
                    key,
                }
            }
            "line" => {
                // LineCount defaults to 1, so MeasureName2 only shows up with LineCount=2
                let series = opt(section, "linecount", vars)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, MAX_SERIES);
                let colors: Vec<Color> = (0..series)
                    .map(|n| {
                        let key = if n == 0 { "linecolor".to_string() } else { format!("linecolor{}", n + 1) };
                        color_opt(section, &key, vars).unwrap_or(WHITE)
                    })
                    .collect();
                let scales: Vec<f64> = (0..series)
                    .map(|n| {
                        let key = if n == 0 { "scale".to_string() } else { format!("scale{}", n + 1) };
                        num(section, &key, vars, 1.0)
                    })
                    .collect();
                let grid_color = if num(section, "horizontallines", vars, 0.0) != 0.0 {
                    Some(
                        color_opt(section, "horizontallinecolor", vars)
                            .unwrap_or(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    )
                } else {
                    None
                };
                let mut graph = Graph::new(section, vars, series);
                carry_history(&mut graph, prev);
                Kind::Line {
                    graph,
                    colors,
                    width: num(section, "linewidth", vars, 1.0).clamp(0.1, 100.0),
                    scales,
                    grid_color,
                }
            }
            "histogram" => {
                // The second measure is optional
                let series = if measure_idx.len() >= 2 { 2 } else { 1 };
                let mut graph = Graph::new(section, vars, series);
                carry_history(&mut graph, prev);
                Kind::Histogram {
                    graph,
                    primary: color_opt(section, "primarycolor", vars)
                        .unwrap_or(Color { r: 0.0, g: 128.0 / 255.0, b: 0.0, a: 1.0 }),
                    secondary: color_opt(section, "secondarycolor", vars)
                        .unwrap_or(Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 }),
                    both: color_opt(section, "bothcolor", vars)
                        .unwrap_or(Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 }),
                }
            }
            other => {
                eprintln!("[{}] unsupported meter type '{}', skipping", section.name, other);
                return None;
            }
        };

        let hidden = num(section, "hidden", vars, 0.0) != 0.0;

        // A rebuilt meter inherits the dynamic source; a new one creates it if asked to
        let dynamic = match prev {
            Some(p) => p.dynamic.clone(),
            None => {
                if num(section, "dynamicvariables", vars, 0.0) != 0.0 {
                    Some(Rc::new(DynSource {
                        name: raw.name.clone(),
                        options: raw.options.clone(),
                        vars: vars.clone(),
                        base_dir: base_dir.to_path_buf(),
                    }))
                } else {
                    None
                }
            }
        };

        // x and y are provisional: Layout::place works out the real position
        let provisional = |p: Pos| match p {
            Pos::Abs(v) => v,
            Pos::Rel { offset, .. } => offset,
        };
        Some(Meter {
            name: raw.name.clone(),
            x_pos,
            y_pos,
            x: provisional(x_pos),
            y: provisional(y_pos),
            solid,
            padding,
            hidden,
            measure_idx,
            kind,
            dynamic,
            // From `raw`: section variables in actions wait until the click
            mouse: MouseActions::read(raw, vars),
        })
    }

    // !ShowMeter / !HideMeter. Like Rainmeter's Meter::Show/Hide, the Hidden option is
    // changed too, or a meter with DynamicVariables=1 would read the old value back
    // on its next update and undo the bang.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        if let Some(src) = self.dynamic.as_mut() {
            let value = if hidden { "1" } else { "0" };
            Rc::make_mut(src).options.insert("hidden".to_string(), value.to_string());
        }
    }

    // !SetVariable: a meter with DynamicVariables=1 reads its own copy of the variables
    // on every update, so the new value goes there too. Other meters keep what they read
    // at load, as in Rainmeter. `name` must already be lowercase, like the table keys.
    pub fn set_variable(&mut self, name: &str, value: &str) {
        if let Some(src) = self.dynamic.as_mut() {
            Rc::make_mut(src).vars.insert(name.to_string(), value.to_string());
        }
    }

    // For !ToggleMeter
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    // Called after the measures update, in order, by update_all
    fn update(
        &mut self,
        measures: &[Box<dyn Measure>],
        values: &Values,
        geom: &Geometry,
    ) {
        // DynamicVariables=1: read the options again with the latest values
        if let Some(src) = self.dynamic.clone() {
            let raw = Section { name: src.name.clone(), options: src.options.clone() };
            if let Some(fresh) = Meter::build(
                &raw,
                &src.vars,
                measures,
                values,
                geom,
                &src.base_dir,
                Some(&*self),
            ) {
                *self = fresh;
            }
        }

        // Graphs record the fresh values
        let graph = match &mut self.kind {
            Kind::Line { graph, .. } | Kind::Histogram { graph, .. } => graph,
            _ => return,
        };
        for (n, series) in graph.history.iter_mut().enumerate() {
            let value = match self.measure_idx.get(n) {
                Some(Some(i)) => measures[*i].percent(),
                _ => 0.0,
            };
            series.push_back(if value.is_finite() { value } else { 0.0 });
            while series.len() > graph.capacity {
                series.pop_front();
            }
        }
    }

    // The meter's own size: W and H if given, otherwise what its content needs
    fn size(&self, measures: &[Box<dyn Measure>]) -> (f64, f64) {
        match &self.kind {
            Kind::Bar { w, h, .. } => (*w, *h),
            Kind::Text { prefix, template, postfix, case, size, face, slant, weight, w, h, .. } => {
                let text = fill_template(prefix, template, postfix, *case, &self.measure_idx, measures);
                let (mw, mh, _) = measure_text(face, *slant, *weight, *size, &text);
                (w.unwrap_or(mw.ceil()), h.unwrap_or(mh.ceil()))
            }
            Kind::Image { pixbuf: Some(p), .. } => (p.width() as f64, p.height() as f64),
            Kind::Image { pixbuf: None, .. } => (0.0, 0.0),
            Kind::Line { graph, .. } | Kind::Histogram { graph, .. } => (graph.w, graph.h),
        }
    }

    // StringAlign of a String meter; every other meter is placed by its top-left corner
    fn align(&self) -> (f64, f64) {
        match &self.kind {
            Kind::Text { align, .. } => *align,
            _ => (0.0, 0.0),
        }
    }

    // Top-left corner of the meter's box. x and y are the anchor point from X and Y:
    // a right-aligned string ends at X, so its box starts a whole width to the left.
    fn origin(&self, w: f64, h: f64) -> (f64, f64) {
        let (ax, ay) = self.align();
        (self.x - ax * w, self.y - ay * h)
    }

    // A content size plus Padding on every side
    fn padded(&self, (w, h): (f64, f64)) -> (f64, f64) {
        let p = &self.padding;
        (w + p.left + p.right, h + p.top + p.bottom)
    }

    // The whole meter: its content plus Padding on every side
    fn outer_size(&self, measures: &[Box<dyn Measure>]) -> (f64, f64) {
        self.padded(self.size(measures))
    }

    // Position and size as other meters see them: a hidden meter has no size
    // but keeps its position
    fn rect(&self, measures: &[Box<dyn Measure>]) -> Rect {
        let (w, h) = if self.hidden { (0.0, 0.0) } else { self.outer_size(measures) };
        let (x, y) = self.origin(w, h);
        Rect { x, y, w, h }
    }

    // Whether (x, y) is on the meter's box, padding included. Hidden meters are never hit.
    fn hit(&self, x: f64, y: f64, measures: &[Box<dyn Measure>]) -> bool {
        if self.hidden {
            return false;
        }
        let r = self.rect(measures);
        x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
    }

    // Bottom-right corner of this meter, used to size the window.
    // Hidden meters still count, because they may be shown later.
    pub fn bounds(&self, measures: &[Box<dyn Measure>]) -> (f64, f64) {
        let (w, h) = self.outer_size(measures);
        let (x, y) = self.origin(w, h);
        (x + w, y + h)
    }

    pub fn draw(&self, cr: &cairo::Context, measures: &[Box<dyn Measure>]) {
        if self.hidden {
            return;
        }

        // Shared by every meter type: SolidColor fills the whole box, padding included,
        // and the content starts inside it at (cx, cy)
        // Asked once here: for a String meter this builds and measures the text
        let (cw, ch) = self.size(measures);
        let (ow, oh) = self.padded((cw, ch));
        let (bx, by) = self.origin(ow, oh);
        if let Some(bg) = &self.solid {
            set_color(cr, bg);
            cr.rectangle(bx, by, ow, oh);
            let _ = cr.fill();
        }
        let (cx, cy) = (bx + self.padding.left, by + self.padding.top);

        match &self.kind {
            Kind::Text { prefix, template, postfix, case, size, color, face, slant, weight, .. } => {
                let text = fill_template(prefix, template, postfix, *case, &self.measure_idx, measures);
                cr.select_font_face(face, *slant, *weight);
                cr.set_font_size(*size);
                set_color(cr, color);
                // The content area comes from W/H (or the text's own size); the text is
                // aligned inside it, which only matters when W or H is bigger than the text
                let (ax, ay) = self.align();
                let (tw, th, ascent) = measure_text(face, *slant, *weight, *size, &text);
                let tx = cx + ax * (cw - tw);
                let ty = cy + ay * (ch - th);
                // Cairo draws text from its baseline, so move down by the ascent
                cr.move_to(tx, ty + ascent);
                let _ = cr.show_text(&text);
            }
            Kind::Bar { w, h, color, vertical } => {
                let fraction = match self.measure_idx.first() {
                    Some(Some(i)) => measures[*i].percent(),
                    _ => 0.0,
                };
                set_color(cr, color);
                if *vertical {
                    let filled = *h * fraction;
                    cr.rectangle(cx, cy + *h - filled, *w, filled);
                } else {
                    cr.rectangle(cx, cy, *w * fraction, *h);
                }
                let _ = cr.fill();
            }
            Kind::Image { pixbuf: Some(p), alpha, .. } => {
                let _ = cr.save();
                cr.set_source_pixbuf(p, cx, cy);
                let _ = cr.paint_with_alpha(*alpha);
                let _ = cr.restore();
            }
            Kind::Image { pixbuf: None, .. } => {}
            Kind::Line { graph, colors, width, scales, grid_color } => {
                let _ = cr.save();
                // Everything below stays inside the graph's rectangle
                cr.rectangle(cx, cy, graph.w, graph.h);
                cr.clip();

                if let Some(gc) = grid_color {
                    // Approximation: marker lines at every quarter of the height
                    set_color(cr, gc);
                    cr.set_line_width(1.0);
                    for q in 1..4 {
                        let y = (cy + graph.h * q as f64 / 4.0).round() + 0.5;
                        cr.move_to(cx, y);
                        cr.line_to(cx + graph.w, y);
                        let _ = cr.stroke();
                    }
                }

                let max = graph.max();
                cr.set_line_width(*width);
                cr.set_line_join(cairo::LineJoin::Round);
                for (n, series) in graph.history.iter().enumerate() {
                    if series.len() < 2 {
                        continue; // one point can't make a line yet
                    }
                    set_color(cr, &colors[n]);
                    for (i, v) in series.iter().rev().enumerate() {
                        let x = cx + graph.col(i) + 0.5;
                        let y = cy + graph.y_of(graph.fraction(*v, scales[n], max));
                        if i == 0 {
                            cr.move_to(x, y);
                        } else {
                            cr.line_to(x, y);
                        }
                    }
                    let _ = cr.stroke();
                }
                let _ = cr.restore();
            }
            Kind::Histogram { graph, primary, secondary, both } => {
                let Some(first) = graph.history.first() else { return };
                let _ = cr.save();
                cr.rectangle(cx, cy, graph.w, graph.h);
                cr.clip();

                // Bar heights in pixels, newest sample first
                let max = graph.max();
                let to_px = |v: &f64| graph.fraction(*v, 1.0, max) * graph.h;
                let p: Vec<f64> = first.iter().rev().map(to_px).collect();
                let has_second = graph.history.len() > 1;
                let s: Vec<f64> = if has_second {
                    graph.history[1].iter().rev().map(to_px).collect()
                } else {
                    Vec::new()
                };

                // A 1-pixel-wide slice from height `from` up to height `to`
                let slice = |col: f64, from: f64, to: f64| {
                    let y = if graph.flip { cy + from } else { cy + graph.h - to };
                    cr.rectangle(cx + col, y, 1.0, to - from);
                };

                // Three passes, one color each: primary-only, secondary-only, overlap
                for pass in 0..3 {
                    let color = match pass {
                        0 => primary,
                        1 => secondary,
                        _ => both,
                    };
                    set_color(cr, color);
                    for (i, hp) in p.iter().enumerate() {
                        let hs = s.get(i).copied().unwrap_or(0.0);
                        let low = hp.min(hs);
                        let (from, to) = match pass {
                            0 => {
                                if has_second { (low, *hp) } else { (0.0, *hp) }
                            }
                            1 => (low, hs),
                            _ => (0.0, low),
                        };
                        if to > from {
                            slice(graph.col(i), from, to);
                        }
                    }
                    let _ = cr.fill();
                }
                let _ = cr.restore();
            }
        }
    }
}

// ---------- layout ----------

// Works out where each meter goes, in skin order. Relative positions (X=10R) use the meter
// before; the table of positions is what [MeterName:X] references read.
pub struct Layout {
    geom: Geometry,
    prev: Rect, // the previous meter's position and size
}

impl Layout {
    pub fn new() -> Layout {
        Layout { geom: Geometry::new(), prev: Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } }
    }

    // Starts from where every meter currently is, so a [Meter:X] reference to a meter
    // further down sees its position from the last update, as in Rainmeter
    fn seeded(meters: &[Meter], measures: &[Box<dyn Measure>]) -> Layout {
        let mut layout = Layout::new();
        for m in meters {
            layout.geom.insert(m.name.to_lowercase(), m.rect(measures));
        }
        layout
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geom
    }

    // Where every meter is now, for [&Meter:X] in a mouse action
    pub fn geometry_of(meters: &[Meter], measures: &[Box<dyn Measure>]) -> Geometry {
        Layout::seeded(meters, measures).geom
    }

    // Sets the meter's real x and y from its X and Y options and the previous meter
    pub fn place(&mut self, m: &mut Meter, measures: &[Box<dyn Measure>]) {
        let prev = self.prev;
        m.x = match m.x_pos {
            Pos::Abs(v) => v,
            Pos::Rel { offset, far } => prev.x + if far { prev.w } else { 0.0 } + offset,
        };
        m.y = match m.y_pos {
            Pos::Abs(v) => v,
            Pos::Rel { offset, far } => prev.y + if far { prev.h } else { 0.0 } + offset,
        };
        let rect = m.rect(measures);
        self.geom.insert(m.name.to_lowercase(), rect);
        // Like Rainmeter, the next meter's r/R counts from this meter's X/Y as written
        // (its anchor), not from its box: a column of centered texts can all use X=0r
        self.prev = Rect { x: m.x, y: m.y, ..rect };
    }
}

// Updates every meter in order: dynamic ones rebuild, graphs record, and positions
// are worked out against the meter before.
pub fn update_all(meters: &mut [Meter], measures: &[Box<dyn Measure>], values: &Values) {
    let mut layout = Layout::seeded(meters, measures);
    for m in meters.iter_mut() {
        m.update(measures, values, layout.geometry());
        layout.place(m, measures);
    }
}

// The action a mouse event at (x, y) runs, as in Rainmeter's Skin::DoAction. Meters are
// checked from the last one (drawn on top) back; the first that is visible, under the
// mouse and has an action for this event wins. Otherwise [Rainmeter]'s own action runs.
pub fn action_at<'a>(
    meters: &'a [Meter],
    skin_mouse: &'a MouseActions,
    measures: &[Box<dyn Measure>],
    x: f64,
    y: f64,
    event: MouseEvent,
) -> Option<(&'a str, &'a str)> {
    meters
        .iter()
        .rev()
        .filter(|m| m.hit(x, y, measures))
        .find_map(|m| m.mouse.get(event).map(|a| (m.name.as_str(), a)))
        .or_else(|| skin_mouse.get(event).map(|a| ("Rainmeter", a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(name: &str, x: Pos, y: Pos, w: f64, h: f64) -> Meter {
        Meter {
            name: name.to_string(),
            x_pos: x,
            y_pos: y,
            x: 0.0,
            y: 0.0,
            solid: None,
            padding: Padding::default(),
            hidden: false,
            measure_idx: Vec::new(),
            kind: Kind::Bar { w, h, color: WHITE, vertical: false },
            dynamic: None,
            mouse: MouseActions::default(),
        }
    }

    // Mouse actions with only a LeftMouseUpAction
    fn on_click(action: &str) -> MouseActions {
        let mut options = HashMap::new();
        options.insert("leftmouseupaction".to_string(), action.to_string());
        MouseActions::read(&Section { name: "T".to_string(), options }, &HashMap::new())
    }

    #[test]
    fn clicks_go_to_the_top_meter_with_an_action() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let abs = Pos::Abs(0.0);
        let mut under = bar("Under", abs, abs, 100.0, 100.0);
        under.mouse = on_click("[!Under]");
        let mut over = bar("Over", abs, abs, 50.0, 50.0);
        over.mouse = on_click("[!Over]");
        // Drawn above Over, but with no action, so clicks pass through it
        let plain = bar("Plain", abs, abs, 30.0, 30.0);
        // Hidden meters are skipped too
        let mut hidden = bar("Hidden", abs, abs, 100.0, 100.0);
        hidden.mouse = on_click("[!Hidden]");
        hidden.hidden = true;

        let mut meters = vec![under, over, plain, hidden];
        let mut layout = Layout::new();
        for m in &mut meters {
            layout.place(m, &measures);
        }
        let skin = on_click("[!Skin]");
        // Only the action text here; the section names are checked by the last line
        let at = |x, y, event| action_at(&meters, &skin, &measures, x, y, event).map(|(_, a)| a);

        assert_eq!(at(10.0, 10.0, MouseEvent::LeftUp), Some("[!Over]"));
        assert_eq!(at(70.0, 70.0, MouseEvent::LeftUp), Some("[!Under]"));
        // Outside every meter: the skin's own action from [Rainmeter]
        assert_eq!(at(150.0, 150.0, MouseEvent::LeftUp), Some("[!Skin]"));
        // Nobody has a RightMouseUpAction
        assert_eq!(at(10.0, 10.0, MouseEvent::RightUp), None);
        // Each action comes with the name of its section
        let named = |x, y| action_at(&meters, &skin, &measures, x, y, MouseEvent::LeftUp).map(|(n, _)| n);
        assert_eq!(named(10.0, 10.0), Some("Over"));
        assert_eq!(named(150.0, 150.0), Some("Rainmeter"));
    }

    fn rel(offset: f64, far: bool) -> Pos {
        Pos::Rel { offset, far }
    }

    #[test]
    fn positions_are_read_from_text() {
        assert_eq!(parse_pos("10"), Ok(Pos::Abs(10.0)));
        assert_eq!(parse_pos(" -5 "), Ok(Pos::Abs(-5.0)));
        assert_eq!(parse_pos("10R"), Ok(rel(10.0, true)));
        assert_eq!(parse_pos("0r"), Ok(rel(0.0, false)));
        assert_eq!(parse_pos("-5R"), Ok(rel(-5.0, true)));
        assert_eq!(parse_pos("R"), Ok(rel(0.0, true)));
        assert_eq!(parse_pos("(2 + 3)R"), Ok(rel(5.0, true)));
        assert!(parse_pos("abcR").is_err());
        assert!(parse_pos("").is_err());
    }

    #[test]
    fn relative_positions_follow_the_previous_meter() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        let mut a = bar("A", Pos::Abs(10.0), Pos::Abs(20.0), 100.0, 30.0);
        let mut b = bar("B", rel(5.0, true), rel(0.0, false), 40.0, 10.0);
        let mut c = bar("C", rel(0.0, false), rel(8.0, true), 10.0, 10.0);
        layout.place(&mut a, &measures);
        layout.place(&mut b, &measures);
        layout.place(&mut c, &measures);
        assert_eq!((a.x, a.y), (10.0, 20.0));
        assert_eq!((b.x, b.y), (115.0, 20.0)); // 10 + 100 + 5, same top as A
        assert_eq!((c.x, c.y), (115.0, 38.0)); // same left as B, 8 below it: 20 + 10 + 8
    }

    #[test]
    fn hidden_meters_keep_a_position_but_no_size() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        let mut a = bar("A", Pos::Abs(0.0), Pos::Abs(0.0), 50.0, 50.0);
        let mut hidden = bar("H", Pos::Abs(0.0), rel(10.0, true), 50.0, 50.0);
        hidden.hidden = true;
        let mut c = bar("C", Pos::Abs(0.0), rel(4.0, true), 50.0, 50.0);
        layout.place(&mut a, &measures);
        layout.place(&mut hidden, &measures);
        layout.place(&mut c, &measures);
        assert_eq!(hidden.y, 60.0); // 0 + 50 + 10
        assert_eq!(c.y, 64.0); // 60 + 0 (hidden has no height) + 4
    }

    #[test]
    fn positions_are_recorded_for_references() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        let mut a = bar("Box", Pos::Abs(10.0), Pos::Abs(20.0), 100.0, 30.0);
        layout.place(&mut a, &measures);
        let r = layout.geometry().get("box").expect("Box should be recorded");
        assert_eq!((r.x, r.y, r.w, r.h), (10.0, 20.0, 100.0, 30.0));
    }

    // A String meter with a fixed W and H, so the tests don't depend on fonts
    fn text(name: &str, x: Pos, y: Pos, align: &str) -> Meter {
        let mut m = bar(name, x, y, 0.0, 0.0);
        m.kind = Kind::Text {
            prefix: String::new(),
            template: "hi".to_string(),
            postfix: String::new(),
            case: Case::None,
            size: 12.0,
            color: WHITE,
            face: "Sans".to_string(),
            slant: cairo::FontSlant::Normal,
            weight: cairo::FontWeight::Normal,
            w: Some(190.0),
            h: Some(14.0),
            align: parse_align(align).unwrap(),
        };
        m
    }

    #[test]
    fn prefix_and_postfix_are_not_filled_in() {
        // %1 is replaced in the template only; an unset measure gives empty text
        let text = fill_template("%1 = ", "[%1]", " %1", Case::None, &[None], &[]);
        assert_eq!(text, "%1 = [] %1");
    }

    #[test]
    fn string_case_matches_windows() {
        // Expected results come from Windows' LCMapString, which Rainmeter calls
        assert_eq!(parse_case("PROPER"), Some(Case::Proper));
        assert_eq!(parse_case("Title"), None);
        let proper = |s| apply_case(s, Case::Proper);
        assert_eq!(proper("hELLO wORLD"), "Hello World");
        assert_eq!(proper("CPU usage"), "CPU Usage");
        assert_eq!(proper("x-ray 2026_x"), "X-Ray 2026_X");
        assert_eq!(proper("it's rock’n roll"), "It's Rock’n Roll");
        assert_eq!(proper("o'neil mcDONALD"), "O'neil Mcdonald");
        assert_eq!(proper("42abc x1y"), "42Abc X1y");
        assert_eq!(proper("МОСКВА мОСКВА"), "МОСКВА Москва");
        assert_eq!(apply_case("straße été", Case::Upper), "STRAßE ÉTÉ");
        assert_eq!(apply_case("ÉTÉ İ", Case::Lower), "été İ");
        // Prefix and Postfix are included
        assert_eq!(fill_template("cpu: ", "", " %", Case::Upper, &[], &[]), "CPU:  %");
    }

    #[test]
    fn string_without_text_or_measure_is_empty() {
        let mut options = HashMap::new();
        options.insert("meter".to_string(), "String".to_string());
        let section = Section { name: "Empty".to_string(), options };
        let m = Meter::from_section(&section, &HashMap::new(), &[], &HashMap::new(), &HashMap::new(), Path::new("."))
            .expect("a String meter should be built");
        let Kind::Text { template, .. } = &m.kind else { panic!("expected a String meter") };
        assert_eq!(template, "");
    }

    #[test]
    fn string_align_is_read() {
        assert_eq!(parse_align("Left"), Some((0.0, 0.0)));
        assert_eq!(parse_align("right"), Some((1.0, 0.0)));
        assert_eq!(parse_align("CENTERCENTER"), Some((0.5, 0.5)));
        assert_eq!(parse_align("RightBottom"), Some((1.0, 1.0)));
        assert_eq!(parse_align("LeftTop"), Some((0.0, 0.0)));
        assert_eq!(parse_align("Middle"), None);
        assert_eq!(parse_align("RightLeft"), None);
    }

    #[test]
    fn aligned_text_is_placed_around_its_anchor() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        // illustro's value column: X=200 W=190 StringAlign=Right -> the box is 10..200
        let mut right = text("Right", Pos::Abs(200.0), Pos::Abs(40.0), "Right");
        // Centered on X and Y
        let mut center = text("Center", Pos::Abs(100.0), Pos::Abs(100.0), "CenterCenter");
        layout.place(&mut right, &measures);
        layout.place(&mut center, &measures);
        let r = layout.geometry().get("right").unwrap();
        assert_eq!((r.x, r.y), (10.0, 40.0));
        let c = layout.geometry().get("center").unwrap();
        assert_eq!((c.x, c.y), (5.0, 93.0)); // 100 - 190/2, 100 - 14/2
        assert_eq!(center.bounds(&measures), (195.0, 107.0));
    }

    #[test]
    fn relative_positions_count_from_the_anchor() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        let mut title = text("Title", Pos::Abs(100.0), Pos::Abs(10.0), "Center");
        let mut next = text("Next", rel(0.0, false), rel(4.0, true), "Center");
        layout.place(&mut title, &measures);
        layout.place(&mut next, &measures);
        // X=0r keeps the same center line; Y=4R goes below the title's box
        assert_eq!((next.x, next.y), (100.0, 28.0)); // 10 + 14 + 4
    }

    #[test]
    fn padding_is_read() {
        let p = |l, t, r, b| Padding { left: l, top: t, right: r, bottom: b };
        assert_eq!(parse_padding("5,6,7,8"), Ok(p(5.0, 6.0, 7.0, 8.0)));
        assert_eq!(parse_padding("5, 6"), Ok(p(5.0, 6.0, 0.0, 0.0)));
        assert_eq!(parse_padding("(2*3),,1,"), Ok(p(6.0, 0.0, 1.0, 0.0)));
        assert_eq!(parse_padding("-5,1e9,0,0"), Ok(p(0.0, MAX_PADDING, 0.0, 0.0)));
        assert!(parse_padding("5,abc,0,0").is_err());
    }

    #[test]
    fn padding_makes_the_meter_bigger() {
        let measures: Vec<Box<dyn Measure>> = Vec::new();
        let mut layout = Layout::new();
        let mut a = bar("A", Pos::Abs(10.0), Pos::Abs(20.0), 100.0, 10.0);
        a.padding = parse_padding("5,5,5,5").unwrap();
        let mut b = bar("B", Pos::Abs(10.0), rel(0.0, true), 100.0, 10.0);
        layout.place(&mut a, &measures);
        layout.place(&mut b, &measures);
        let r = layout.geometry().get("a").unwrap();
        assert_eq!((r.w, r.h), (110.0, 20.0)); // what [A:W] and [A:H] see
        assert_eq!(a.bounds(&measures), (120.0, 40.0)); // for the window size
        assert_eq!(b.y, 40.0); // Y=0R starts below A's padding: 20 + 20
    }
}