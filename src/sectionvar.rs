// Section variables: [MeasureName], [MeasureName:1], [MeasureName:/1024,2], [MeasureName:%],
// and for meters [MeterName:X], [MeterName:W], ...
use crate::ini;
use crate::measure::{fraction_in, Sample, Values};
use std::collections::HashMap;

const MAX_EXPANDED: usize = 64 * 1024; // longest result we will build
const MAX_REF_LEN: usize = 256; // a reference longer than this is just text

// A meter's real position and size, as section variables see it
#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

// Keys are lowercase meter names
pub type Geometry = HashMap<String, Rect>;

enum Base {
    Value,
    Min,
    Max,
}

// Measure references only
pub fn expand(text: &str, values: &Values) -> String {
    expand_with(text, values, &Geometry::new())
}

// Replaces every [Name...] that names a measure or a meter. Anything else is left exactly
// as written. The work stays linear even for hostile input.
pub fn expand_with(text: &str, values: &Values, geom: &Geometry) -> String {
    // Nothing to expand. An option longer than the cap is also left exactly as written,
    // so a huge value is never cut short, and hostile input can't cost us much work.
    if !text.contains('[') || text.len() > MAX_EXPANDED {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut next_close: Option<usize> = None; // index of the next ']' after i
    let mut none_left = false; // true once we know there is no ']' anywhere ahead

    while i < chars.len() && out.len() <= MAX_EXPANDED {
        if chars[i] == '[' && !none_left {
            if next_close.map_or(true, |c| c <= i) {
                match chars[i + 1..].iter().position(|&c| c == ']') {
                    Some(offset) => next_close = Some(i + 1 + offset),
                    None => {
                        none_left = true;
                        next_close = None;
                    }
                }
            }
            if let Some(end) = next_close {
                if end - i <= MAX_REF_LEN {
                    let inner: String = chars[i + 1..end].iter().collect();
                    if !inner.contains('[') {
                        if let Some(replacement) = lookup(&inner, values, geom) {
                            out.push_str(&replacement);
                            i = end + 1;
                            continue;
                        }
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    if out.len() > MAX_EXPANDED {
        let mut cut = MAX_EXPANDED;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

// What "Name" or "Name:params" turns into, or None if it isn't a measure or meter we know
fn lookup(inner: &str, values: &Values, geom: &Geometry) -> Option<String> {
    let (name, params) = match inner.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (inner, None),
    };
    let key = name.trim().to_lowercase();
    if let Some(sample) = values.get(&key) {
        return measure_value(sample, params);
    }
    if let Some(rect) = geom.get(&key) {
        return meter_value(rect, params);
    }
    None
}

// [MeterName:X], :Y, :W, :H, :XW, :YH. A meter has no value without a parameter.
// Like Rainmeter, the result is always a whole number.
fn meter_value(rect: &Rect, params: Option<&str>) -> Option<String> {
    let number = match params?.trim().to_lowercase().as_str() {
        "x" => rect.x,
        "y" => rect.y,
        "w" => rect.w,
        "h" => rect.h,
        "xw" => rect.x + rect.w,
        "yh" => rect.y + rect.h,
        _ => return None,
    };
    Some(format!("{}", number.round() as i64))
}

fn measure_value(sample: &Sample, params: Option<&str>) -> Option<String> {
    // No parameters: the measure's string value
    let Some(params) = params else {
        return Some(sample.text.clone());
    };

    let mut base = Base::Value;
    let mut percent = false;
    let mut divisor: Option<f64> = None;
    let mut decimals: Option<usize> = None;
    for token in params.split(',') {
        let t = token.trim();
        if t.is_empty() {
            continue; // "[Name:]" means the number value
        } else if t == "%" {
            percent = true;
        } else if let Some(n) = t.strip_prefix('/') {
            let d = n.trim().parse::<u32>().ok().filter(|&d| d > 0)?;
            divisor = Some(f64::from(d));
        } else if t.eq_ignore_ascii_case("minvalue") {
            base = Base::Min;
        } else if t.eq_ignore_ascii_case("maxvalue") {
            base = Base::Max;
        } else if let Ok(d) = t.parse::<usize>() {
            decimals = Some(d.min(10));
        } else {
            return None; // a parameter we don't support: leave the text alone
        }
    }

    let mut number = match base {
        Base::Min => sample.min,
        Base::Max => sample.max,
        Base::Value => {
            if percent {
                fraction_in(sample.value, sample.min, sample.max) * 100.0
            } else {
                sample.value
            }
        }
    };
    if let Some(d) = divisor {
        number /= d;
    }
    Some(format_decimals(number, decimals))
}

// With a count: exactly that many decimals. Without: up to ten, trailing zeros removed.
fn format_decimals(v: f64, decimals: Option<usize>) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    match decimals {
        Some(d) => format!("{:.*}", d, v),
        None => {
            let s = format!("{:.10}", v);
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s.is_empty() || s == "-0" {
                "0".to_string()
            } else {
                s.to_string()
            }
        }
    }
}

// Expands the section variables in every option value of a section.
// #Variables# are resolved first, as in Rainmeter, so [#Name:#Digits#]-style mixes work.
// Then nested [#Variables], so [Measure[#N]] names a measure by the time it's looked up.
pub fn expand_options(
    options: &HashMap<String, String>,
    vars: &HashMap<String, String>,
    values: &Values,
    geom: &Geometry,
) -> HashMap<String, String> {
    options
        .iter()
        .map(|(k, v)| {
            let value = if v.contains('[') {
                expand_with(&ini::resolve_nested(&ini::resolve(v, vars), vars), values, geom)
            } else {
                v.clone()
            };
            (k.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_values() -> Values {
        let mut v = Values::new();
        v.insert(
            "a".to_string(),
            Sample { value: 12.6, min: 0.0, max: 100.0, text: "text_a".to_string() },
        );
        v.insert(
            "b".to_string(),
            Sample { value: 50.0, min: 0.0, max: 200.0, text: "b".to_string() },
        );
        v
    }

    #[test]
    fn string_and_number_values() {
        let v = sample_values();
        assert_eq!(expand("[a]", &v), "text_a");
        assert_eq!(expand("[A]", &v), "text_a");
        assert_eq!(expand("[a:]", &v), "12.6");
        assert_eq!(expand("[a:1]", &v), "12.6");
        assert_eq!(expand("[a:0]", &v), "13");
        assert_eq!(expand("[a:/2,2]", &v), "6.30");
        assert_eq!(expand("[a:MaxValue]", &v), "100");
        assert_eq!(expand("[b:%]", &v), "25");
    }

    #[test]
    fn works_inside_other_text() {
        let v = sample_values();
        assert_eq!(expand("X=([a:] * 2)", &v), "X=(12.6 * 2)");
        assert_eq!(expand("[a][a]", &v), "text_atext_a");
        assert_eq!(expand("[[a]]", &v), "[text_a]");
    }

    #[test]
    fn unknown_things_are_left_alone() {
        let v = sample_values();
        assert_eq!(expand("[zzz]", &v), "[zzz]");
        assert_eq!(expand("[a:bogus]", &v), "[a:bogus]");
        assert_eq!(expand("[a", &v), "[a");
        assert_eq!(expand("no brackets", &v), "no brackets");
    }

    #[test]
    fn meter_positions_and_sizes() {
        let v = sample_values();
        let mut g = Geometry::new();
        g.insert("box".to_string(), Rect { x: 10.0, y: 20.4, w: 100.0, h: 30.0 });
        assert_eq!(expand_with("[Box:X] [Box:Y] [Box:W] [Box:H]", &v, &g), "10 20 100 30");
        assert_eq!(expand_with("[Box:XW] [Box:YH]", &v, &g), "110 50");
        // No parameter, or one meters don't have: left alone
        assert_eq!(expand_with("[Box] [Box:Z] [Box:X,Y]", &v, &g), "[Box] [Box:Z] [Box:X,Y]");
        // Typical use: inside a formula
        let text = expand_with("([Box:X] + [Box:W])", &v, &g);
        assert_eq!(crate::formula::parse_number(&text), Ok(110.0));
    }

    #[test]
    fn hostile_input_stays_fast_and_bounded() {
        let v = sample_values();
        // Just under the cap, so the real scanning code runs on every character
        let opens = "[".repeat(60_000);
        assert_eq!(expand(&opens, &v), opens);
        let opens_then_close = format!("{}]", opens);
        assert_eq!(expand(&opens_then_close, &v), opens_then_close);
        // Expansion that would grow without limit is cut off at the cap
        let many = "[a]".repeat(20_000);
        assert!(expand(&many, &v).len() <= MAX_EXPANDED);
        // Values over the cap are returned untouched, not truncated
        let huge = format!("{}[a]", "x".repeat(MAX_EXPANDED));
        assert_eq!(expand(&huge, &v), huge);
    }
}