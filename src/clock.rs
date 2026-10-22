use crate::measure::Measure;
use chrono::{DateTime, FixedOffset, Local, Timelike, Utc};
use std::fmt::Write;

const DEFAULT_FORMAT: &str = "%H:%M:%S";

pub struct TimeMeasure {
    name: String,
    format: String,
    offset: Option<FixedOffset>, // None = this computer's local time
    text: String,
    seconds: f64,
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
        let mut format = format.replace("%#", "%-");

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
            text: String::new(),
            seconds: 0.0,
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
        self.seconds = now.num_seconds_from_midnight() as f64;
        self.text = render(&self.format, &now).unwrap_or_default();
    }

    // Like Rainmeter, the number is seconds since midnight; the text is the formatted time
    fn value(&self) -> f64 {
        self.seconds
    }

    // Seconds since midnight, so a bar on this measure shows how far through the day it is
    fn range(&self) -> (f64, f64) {
        (0.0, 86400.0)
    }

    fn text(&self) -> String {
        self.text.clone()
    }
}