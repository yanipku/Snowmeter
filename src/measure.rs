use crate::{formula, sectionvar};
use std::collections::HashMap;

// A measure's latest values, as seen by Calc formulas and section variables
pub struct Sample {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub text: String, // the string value, what [MeasureName] expands to
}

// Keys are lowercase measure names
pub type Values = HashMap<String, Sample>;

// Where `value` sits inside min..max, from 0.0 to 1.0
pub fn fraction_in(value: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    if span == 0.0 {
        return 0.0;
    }
    let p = (value - min) / span;
    if p.is_finite() { p.clamp(0.0, 1.0) } else { 0.0 }
}

pub trait Measure {
    fn name(&self) -> &str;
    fn update(&mut self);

    // Like update(), for measures that read other measures' values (Calc).
    // Everything else just updates itself.
    fn update_with(&mut self, _values: &Values) {
        self.update();
    }

    fn value(&self) -> f64;

    // The MinValue..MaxValue range that value() is measured against.
    // Our built-in measures report percentages, so the default is 0..100.
    fn range(&self) -> (f64, f64) {
        (0.0, 100.0)
    }

    // Where value() sits inside its range, from 0.0 to 1.0
    fn percent(&self) -> f64 {
        let (min, max) = self.range();
        fraction_in(self.value(), min, max)
    }

    // Default implementation: measures can override it
    fn text(&self) -> String {
        format!("{:.1}", self.value())
    }
}

// Wraps a measure so a skin's MinValue / MaxValue override its own range
pub struct Ranged {
    inner: Box<dyn Measure>,
    min: Option<f64>,
    max: Option<f64>,
    // DynamicVariables=1: the option text is read again on every update
    min_src: Option<String>,
    max_src: Option<String>,
    complained: bool, // so a bad value is reported once, not every update
}

impl Ranged {
    // Fixed numbers. None means "keep the measure's own value for that end".
    pub fn new(inner: Box<dyn Measure>, min: Option<f64>, max: Option<f64>) -> Ranged {
        Ranged { inner, min, max, min_src: None, max_src: None, complained: false }
    }

    // Option text that may contain [Name] references, evaluated on every update
    pub fn dynamic(
        inner: Box<dyn Measure>,
        min_src: Option<String>,
        max_src: Option<String>,
    ) -> Ranged {
        Ranged { inner, min: None, max: None, min_src, max_src, complained: false }
    }
}

fn read_number(src: &str, values: &Values) -> Result<f64, String> {
    formula::parse_number(&sectionvar::expand(src, values))
}

impl Measure for Ranged {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn update(&mut self) {
        self.inner.update();
    }

    fn update_with(&mut self, values: &Values) {
        self.inner.update_with(values);

        let mut failed: Option<String> = None;
        if let Some(src) = &self.min_src {
            match read_number(src, values) {
                Ok(v) => self.min = Some(v),
                Err(e) => failed = Some(e),
            }
        }
        if let Some(src) = &self.max_src {
            match read_number(src, values) {
                Ok(v) => self.max = Some(v),
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) if !self.complained => {
                eprintln!(
                    "[{}] cannot read MinValue/MaxValue: {} (keeping the last good value)",
                    self.inner.name(),
                    e
                );
                self.complained = true;
            }
            None => self.complained = false,
            _ => {}
        }
    }

    fn value(&self) -> f64 {
        self.inner.value()
    }

    // Asked each time, so a measure with a moving range (Calc) keeps moving
    fn range(&self) -> (f64, f64) {
        let (own_min, own_max) = self.inner.range();
        (self.min.unwrap_or(own_min), self.max.unwrap_or(own_max))
    }

    fn text(&self) -> String {
        self.inner.text()
    }
}

fn sample_of(m: &dyn Measure) -> Sample {
    let (min, max) = m.range();
    Sample { value: m.value(), min, max, text: m.text() }
}

// Updates every measure in order, like Rainmeter does, and returns everyone's fresh values
pub fn update_all(measures: &mut [Box<dyn Measure>]) -> Values {
    // Start from each measure's current (old) value, so a Calc can read a measure defined below it
    let mut values: Values = HashMap::new();
    for m in measures.iter() {
        values.insert(m.name().to_lowercase(), sample_of(&**m));
    }
    // Then update in order: each fresh value replaces the old one for the measures after it
    for m in measures.iter_mut() {
        m.update_with(&values);
        values.insert(m.name().to_lowercase(), sample_of(&**m));
    }
    values
}
