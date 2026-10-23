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

// For [&Name] outside meter and measure options (mouse actions)
pub fn lookup_section(inner: &str, values: &Values, geom: &Geometry) -> Option<String> {
    lookup(inner, values, geom)
}

// What "Name" or "Name:params" turns into, or None if it isn't a measure or meter we know.
// Like Rainmeter, the parameters start after the *last* colon, so in
// [Name:MaxValue:/1024,1] the name part is "Name:MaxValue".
fn lookup(inner: &str, values: &Values, geom: &Geometry) -> Option<String> {
    let (name, params) = match inner.rsplit_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (inner, None),
    };
    let key = name.trim().to_lowercase();
    if let Some(sample) = values.get(&key) {
        return measure_value(sample, Base::Value, params);
    }
    if let Some(rect) = geom.get(&key) {
        return meter_value(rect, params);
    }
    // [Name:MaxValue:/scale,dec] and [Name:MinValue:...]. Only for parameters that
    // don't start with a letter, as in Rainmeter ([Name:MaxValue:MaxValue] isn't valid).
    let params = params?;
    if params.starts_with(|c: char| c.is_alphabetic()) {
        return None;
    }
    let (name, keyword) = key.rsplit_once(':')?;
    let base = match keyword {
        "maxvalue" => Base::Max,
        "minvalue" => Base::Min,
        _ => return None,
    };
    measure_value(values.get(name.trim())?, base, Some(params))
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

// Follows Rainmeter's GetSectionVariable. The parameters are one of:
//   (empty) or dec   the number, with dec decimals
//   %[,dec]          percent of the range (not with MaxValue/MinValue)
//   /scale[,dec]     the number divided by scale
//   MaxValue / MinValue
// Anything else returns None, so the text is left as written.
fn measure_value(sample: &Sample, mut base: Base, params: Option<&str>) -> Option<String> {
    // No parameters: the measure's string value
    let Some(params) = params else {
        return Some(sample.text.clone());
    };

    // [Name:MaxValue], [Name:MinValue], [Name:TimeStamp]: nothing may follow the keyword
    let mut rest = params;
    if params.starts_with(|c: char| c.is_alphabetic()) {
        base = match params.to_lowercase().as_str() {
            "maxvalue" => Base::Max,
            "minvalue" => Base::Min,
            // Time measures only, always a whole number of seconds since 1601
            "timestamp" => return sample.timestamp.map(|t| format!("{}", t as i64)),
            "escaperegexp" => return Some(escape_regexp(&sample.text)),
            "encodeurl" => return Some(encode_url(&sample.text)),
            _ => return None,
        };
        rest = "";
    }

    let has_decimals = rest.contains(',');
    let mut percent = false;
    let mut scale = 1.0;
    let decimals_text = if let Some(after) = rest.strip_prefix('%') {
        if !matches!(base, Base::Value) {
            return None; // '%' can't be used with MaxValue/MinValue
        }
        percent = true;
        has_decimals.then(|| after.split_once(',').map_or("", |(_, d)| d))
    } else if let Some(after) = rest.strip_prefix('/') {
        scale = f64::from(leading_int(after).filter(|&s| s != 0)?);
        has_decimals.then(|| after.split_once(',').map_or("", |(_, d)| d))
    } else {
        if has_decimals {
            return None; // [Name:,2] and [Name:2,2] aren't valid
        }
        Some(rest)
    };
    // Decimals given: exactly that many (0-32). Not given: trailing zeros are removed.
    let decimals = decimals_text
        .map(str::trim_start)
        .filter(|d| !d.is_empty())
        .map(|d| leading_int(d).unwrap_or(0).clamp(0, 32) as usize);

    let number = match base {
        Base::Max => sample.max / scale,
        Base::Min => sample.min / scale,
        Base::Value if percent => fraction_in(sample.value, sample.min, sample.max) * 100.0,
        Base::Value => sample.value / scale,
    };
    Some(format_decimals(number, decimals))
}

// [Measure:EscapeRegExp]: a '\' before each character that means something in a regular
// expression. Like Rainmeter, ']' and '}' are left alone.
fn escape_regexp(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\^$|()[{.+*?".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// [Measure:EncodeUrl]: every UTF-8 byte except 0-9 A-Z a-z - . _ ~ becomes %XX
fn encode_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// The whole number at the start of `s`, the way C's wcstol reads it:
// spaces skipped, an optional sign, then digits. Anything after them is ignored.
fn leading_int(s: &str) -> Option<i32> {
    let s = s.trim_start();
    let sign_len = usize::from(s.starts_with(['+', '-']));
    let digits = s[sign_len..].chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    // Too many digits for an i32: wcstol gives the largest value, so do the same
    let text = &s[..sign_len + digits];
    Some(text.parse().unwrap_or(if text.starts_with('-') { i32::MIN } else { i32::MAX }))
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
// Then nested [#Variables] and [&Measures], so [Measure[#N]] names a measure by the time
// it's looked up.
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
                let nested = ini::resolve_nested_with(&ini::resolve(v, vars), vars, &|name| {
                    lookup(name, values, geom)
                });
                expand_with(&nested, values, geom)
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
            Sample { value: 12.6, min: 0.0, max: 100.0, text: "text_a".to_string(), timestamp: None },
        );
        v.insert(
            "b".to_string(),
            Sample { value: 50.0, min: 0.0, max: 200.0, text: "b".to_string(), timestamp: None },
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
    fn timestamp_only_for_time_measures() {
        let mut v = sample_values();
        v.insert(
            "clock".to_string(),
            Sample { value: 14.0, min: 0.0, max: 1.0, text: "14".to_string(), timestamp: Some(13_400_000_000.0) },
        );
        assert_eq!(expand("[Clock:TimeStamp]", &v), "13400000000");
        assert_eq!(expand("[clock:timestamp]", &v), "13400000000");
        // Not a Time measure, or something after the keyword: left as written
        assert_eq!(expand("[a:TimeStamp]", &v), "[a:TimeStamp]");
        assert_eq!(expand("[clock:TimeStamp,2]", &v), "[clock:TimeStamp,2]");
    }

    #[test]
    fn escape_and_encode() {
        assert_eq!(escape_regexp("1.5 (max)"), r"1\.5 \(max\)");
        assert_eq!(escape_regexp(r"\^$|()[]{}.+*?"), r"\\\^\$\|\(\)\[]\{}\.\+\*\?");
        assert_eq!(encode_url("a b"), "a%20b");
        assert_eq!(encode_url("é"), "%C3%A9");
        assert_eq!(encode_url("Az09-._~/?&="), "Az09-._~%2F%3F%26%3D");
        // Through a section variable, on the measure's text
        let v = sample_values();
        assert_eq!(expand("[a:EscapeRegExp]", &v), "text_a");
        assert_eq!(expand("[a:encodeurl]", &v), "text_a");
        assert_eq!(expand("[a:EncodeUrl,1]", &v), "[a:EncodeUrl,1]");
    }

    #[test]
    fn rainmeter_grammar() {
        let v = sample_values();
        // MaxValue/MinValue with a scale go before the last colon
        assert_eq!(expand("[a:MaxValue:/2,1]", &v), "50.0");
        assert_eq!(expand("[b:minvalue:/2]", &v), "0");
        assert_eq!(expand("[b:%,1]", &v), "25.0");
        assert_eq!(expand("[a:/-2]", &v), "-6.3");
        // Spaces before a number are skipped, as C's wcstol does
        assert_eq!(expand("[a: 2]", &v), "12.60");
        // Forms Rainmeter rejects are left as written
        for bad in ["[a:MaxValue:%]", "[a:,2]", "[a:2,2]", "[a:/0]", "[a:/x]", "[a:MaxValue:MaxValue]"] {
            assert_eq!(expand(bad, &v), bad);
        }
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
    fn nested_section_variables() {
        let v = sample_values();
        let mut g = Geometry::new();
        g.insert("box".to_string(), Rect { x: 10.0, y: 20.0, w: 100.0, h: 30.0 });
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), "a".to_string());
        vars.insert("colorb".to_string(), "red".to_string());
        let expand_one = |text: &str| {
            let options = HashMap::from([("text".to_string(), text.to_string())]);
            expand_options(&options, &vars, &v, &g)["text"].clone()
        };
        assert_eq!(expand_one("[&a]"), "text_a");
        assert_eq!(expand_one("[&a:1]"), "12.6");
        assert_eq!(expand_one("[&Box:X]"), "10");
        // A measure picks the variable, and a variable picks the measure
        assert_eq!(expand_one("[#Color[&b]]"), "red");
        assert_eq!(expand_one("[&[#N]]"), "text_a");
        // Unknown names stay, and the escape gives the text back
        assert_eq!(expand_one("[&Nope]"), "[&Nope]");
        assert_eq!(expand_one("[&*a*]"), "[&a]");
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