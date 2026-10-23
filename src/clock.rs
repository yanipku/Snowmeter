use crate::measure::Measure;
use chrono::{DateTime, FixedOffset, Local, Utc};
use std::fmt::Write;

const DEFAULT_FORMAT: &str = "%H:%M:%S";

pub struct TimeMeasure {
    name: String,
    format: String,
    offset: Option<FixedOffset>, // None = this computer's local time
    has_format: bool,            // the skin set Format=, so the value is read from the text
    text: String,
    value: f64,
    timestamp: f64, // seconds since 1601 at the last update
}

// Seconds between 1601-01-01 (where Windows counts time from) and 1970-01-01 (Linux)
const SECONDS_1601_TO_1970: i64 = 11_644_473_600;

// Rainmeter's timestamp: whole seconds since 1601-01-01, in the measure's own time zone
fn windows_timestamp(dt: &DateTime<FixedOffset>) -> f64 {
    (dt.timestamp() + i64::from(dt.offset().local_minus_utc()) + SECONDS_1601_TO_1970) as f64
}

// The number at the start of `s`, the way C's wcstod reads it: spaces skipped, then
// an optional sign, digits, a '.', more digits and an exponent. 0 if there's no number.
fn leading_number(s: &str) -> f64 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let mut end = usize::from(s.starts_with(['+', '-']));
    let int_end = digits(end);
    let mut seen = int_end > end;
    end = int_end;
    if b.get(end) == Some(&b'.') {
        let frac_end = digits(end + 1);
        seen |= frac_end > end + 1;
        end = frac_end;
    }
    if !seen {
        return 0.0;
    }
    // An exponent counts only if digits follow it ("5e" is just 5)
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(end + 1), Some(b'+' | b'-')));
        let exp_end = digits(end + 1 + sign);
        if exp_end > end + 1 + sign {
            end = exp_end;
        }
    }
    s[..end].parse().unwrap_or(0.0)
}

// Formats a time, returning None if the format string is invalid.
// (chrono's .to_string() would panic on a bad format, and skins are untrusted input.)
fn render(format: &str, dt: &DateTime<FixedOffset>) -> Option<String> {
    let mut out = String::new();
    write!(out, "{}", dt.format(format)).ok()?;
    Some(out)
}

impl TimeMeasure {
    pub fn new(name: &str, format: &str, timezone_hours: Option<f64>) -> Self {
        // Windows strftime writes "no leading zero" as %#d; chrono writes it %-d
        let has_format = !format.is_empty();
        let mut format = if has_format { format.replace("%#", "%-") } else { DEFAULT_FORMAT.to_string() };

        let offset = timezone_hours.and_then(|h| {
            let off = FixedOffset::east_opt((h * 3600.0).round() as i32);
            if off.is_none() {
                eprintln!("[{}] TimeZone {} is out of range, using local time", name, h);
            }
            off
        });

        let mut m = Self {
            name: name.to_string(),
            format: String::new(),
            offset,
            has_format,
            text: String::new(),
            value: 0.0,
            timestamp: 0.0,
        };

        // Test the format once now, so a typo in a skin can't break every update
        if render(&format, &m.now()).is_none() {
            eprintln!("[{}] invalid Format '{}', using {}", name, format, DEFAULT_FORMAT);
            format = DEFAULT_FORMAT.to_string();
        }
        m.format = format;
        m.update();
        m
    }

    fn now(&self) -> DateTime<FixedOffset> {
        match self.offset {
            Some(off) => Utc::now().with_timezone(&off),
            None => Local::now().fixed_offset(),
        }
    }
}

impl Measure for TimeMeasure {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) {
        let now = self.now();
        self.text = render(&self.format, &now).unwrap_or_default();
        self.timestamp = windows_timestamp(&now);
        // Like Rainmeter: with a Format, the number at the start of the text (Format=%H gives 14);
        // without one, the timestamp
        self.value = if self.has_format { leading_number(&self.text) } else { self.timestamp };
    }

    fn timestamp(&self) -> Option<f64> {
        Some(self.timestamp)
    }

    fn value(&self) -> f64 {
        self.value
    }

    // Rainmeter's default range for every measure
    fn range(&self) -> (f64, f64) {
        (0.0, 1.0)
    }

    fn text(&self) -> String {
        self.text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_from_text_like_wcstod() {
        assert_eq!(leading_number("14"), 14.0);
        assert_eq!(leading_number("14:05"), 14.0);
        assert_eq!(leading_number("  -2.5x"), -2.5);
        assert_eq!(leading_number(".5"), 0.5);
        assert_eq!(leading_number("1e3"), 1000.0);
        assert_eq!(leading_number("5e"), 5.0);
        assert_eq!(leading_number("Monday"), 0.0);
        assert_eq!(leading_number(""), 0.0);
    }

    #[test]
    fn timestamp_counts_from_1601_in_local_time() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let epoch = DateTime::from_timestamp(0, 0).unwrap().with_timezone(&utc);
        assert_eq!(windows_timestamp(&epoch), 11_644_473_600.0);
        // One hour east of UTC: the local clock is an hour ahead, and so is the timestamp
        let plus_one = FixedOffset::east_opt(3600).unwrap();
        assert_eq!(windows_timestamp(&epoch.with_timezone(&plus_one)), 11_644_477_200.0);
    }
}
