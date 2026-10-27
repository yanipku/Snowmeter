use crate::ini::{self, Section};
use crate::{formula, sectionvar};
use std::collections::HashMap;

// A measure's latest values, as seen by Calc formulas and section variables
pub struct Sample {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub text: String, // the string value, what [MeasureName] expands to
    pub timestamp: Option<f64>, // Time measures only, for [MeasureName:TimeStamp]
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

// Reads a number option such as MinValue. It may be a (formula).
// Bad values are reported and ignored.
pub fn number_option(
    section: &Section,
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

    // Rainmeter's timestamp (seconds since 1601), for [Measure:TimeStamp]. Only Time has one.
    fn timestamp(&self) -> Option<f64> {
        None
    }

    // Reads the measure's own options again, after !SetOption changed one (Rainmeter's
    // ReadOptions). The measure itself stays, so its state (a CPU sample, a Calc counter)
    // carries on. Default: the measure has no options of its own.
    fn reread(&mut self, _section: &Section, _vars: &HashMap<String, String>) {}

    // The Disabled/Paused state, for the bangs that change it. Only Switch has one;
    // skin::load wraps every measure in a Switch, so through the skin this is always Some.
    fn switch_mut(&mut self) -> Option<&mut Switch> {
        None
    }
}

// Wraps a measure so it can be disabled or paused (Disabled=, Paused=, and bangs such as
// !DisableMeasure). As in Rainmeter's Measure::Update: a paused measure isn't updated and
// keeps its last value; a disabled one isn't updated and its value becomes 0.
// Paused wins when both are set.
// It also keeps the section's options as written, so !SetOption can change one and have
// the measure read them again at the next update.
pub struct Switch {
    inner: Box<dyn Measure>,
    disabled: bool, // change them with set_disabled / set_paused, which keep the options in step
    paused: bool,
    zeroed: bool, // set when a disabled update made the value 0; cleared by a real update
    source: Option<Section>,      // the options as written (None only in tests)
    vars: HashMap<String, String>, // this measure's copy of the variables
    reread_once: bool,            // read the options again at the next update
}

impl Switch {
    pub fn new(inner: Box<dyn Measure>, disabled: bool, paused: bool) -> Switch {
        let (source, vars) = (None, HashMap::new());
        Switch { inner, disabled, paused, zeroed: false, source, vars, reread_once: false }
    }

    // The measure's section and variables, kept for rereading
    pub fn with_source(mut self, section: &Section, vars: &HashMap<String, String>) -> Switch {
        self.source = Some(section.clone());
        self.vars = vars.clone();
        self
    }

    // !SetOption: changes one option as written, and rereads them all at the next update
    // (Rainmeter's Skin::SetOption). An empty value removes the option, so its default
    // applies again.
    pub fn set_option(&mut self, key: &str, value: &str) {
        let Some(src) = self.source.as_mut() else { return };
        let key = key.to_lowercase(); // the table keys are lowercase
        if value.is_empty() {
            src.options.remove(&key);
        } else {
            src.options.insert(key, value.to_string());
        }
        self.reread_once = true;
    }

    // !EnableMeasure / !DisableMeasure and the pause bangs. As in Rainmeter's
    // Measure::Disable() and the like, the option is changed too, or the next reread
    // (after a !SetOption) would read the old value back and undo the bang.
    pub fn set_disabled(&mut self, on: bool) {
        self.disabled = on;
        self.write_flag("disabled", on);
    }

    pub fn set_paused(&mut self, on: bool) {
        self.paused = on;
        self.write_flag("paused", on);
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    fn write_flag(&mut self, key: &str, on: bool) {
        if let Some(src) = self.source.as_mut() {
            src.options.insert(key.to_string(), if on { "1" } else { "0" }.to_string());
        }
    }

    // !SetVariable: seen the next time the options are read. `name` must be lowercase.
    pub fn set_variable(&mut self, name: &str, value: &str) {
        self.vars.insert(name.to_string(), value.to_string());
    }

    // After !SetOption: reads the options again, with [Measure] references filled in from
    // the latest values. Disabled/Paused first, then the measure's own options.
    fn reread_if_asked(&mut self, values: &Values) {
        if !std::mem::take(&mut self.reread_once) {
            return;
        }
        let Some(src) = &self.source else { return };
        let options = sectionvar::expand_options(&src.options, &self.vars, values, &Default::default());
        let section = Section { name: src.name.clone(), options };
        let flag = |key: &str| number_option(&section, key, &self.vars).unwrap_or(0.0) != 0.0;
        (self.disabled, self.paused) = (flag("disabled"), flag("paused"));
        self.inner.reread(&section, &self.vars);
    }

    // Whether this update should reach the measure inside
    fn runs(&mut self) -> bool {
        if self.paused {
            return false;
        }
        self.zeroed = self.disabled;
        !self.disabled
    }
}

impl Measure for Switch {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn update(&mut self) {
        self.reread_if_asked(&Values::new());
        if self.runs() {
            self.inner.update();
        }
    }

    // As in Rainmeter's Measure::Update, the options are read before the Paused check
    fn update_with(&mut self, values: &Values) {
        self.reread_if_asked(values);
        if self.runs() {
            self.inner.update_with(values);
        }
    }

    fn value(&self) -> f64 {
        if self.zeroed { 0.0 } else { self.inner.value() }
    }

    fn range(&self) -> (f64, f64) {
        self.inner.range()
    }

    fn text(&self) -> String {
        if self.zeroed { "0".to_string() } else { self.inner.text() }
    }

    fn timestamp(&self) -> Option<f64> {
        if self.zeroed { None } else { self.inner.timestamp() }
    }

    fn switch_mut(&mut self) -> Option<&mut Switch> {
        Some(self)
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
    // No MinValue/MaxValue yet: read_range() sets them
    pub fn wrap(inner: Box<dyn Measure>) -> Ranged {
        Ranged { inner, min: None, max: None, min_src: None, max_src: None, complained: false }
    }

    // Reads MinValue/MaxValue, at load and after !SetOption. A missing one (None) keeps
    // the measure's own value for that end. With DynamicVariables=1 their text is kept,
    // [Name] references and all, and read again on every update; otherwise they are
    // fixed numbers.
    pub fn read_range(&mut self, section: &Section, vars: &HashMap<String, String>) {
        let dynamic = number_option(section, "dynamicvariables", vars).unwrap_or(0.0) != 0.0;
        if dynamic {
            let source = |key: &str| {
                section.options.get(key).map(|v| ini::resolve_nested(&ini::resolve(v, vars), vars))
            };
            self.min_src = source("minvalue");
            self.max_src = source("maxvalue");
            (self.min, self.max) = (None, None); // until the next update reads them
        } else {
            (self.min_src, self.max_src) = (None, None);
            self.min = number_option(section, "minvalue", vars);
            self.max = number_option(section, "maxvalue", vars);
        }
        self.complained = false;
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

    fn timestamp(&self) -> Option<f64> {
        self.inner.timestamp()
    }

    fn reread(&mut self, section: &Section, vars: &HashMap<String, String>) {
        self.read_range(section, vars);
        self.inner.reread(section, vars);
    }
}

fn sample_of(m: &dyn Measure) -> Sample {
    let (min, max) = m.range();
    Sample { value: m.value(), min, max, text: m.text(), timestamp: m.timestamp() }
}

// Updates every measure in order, like Rainmeter does, and returns everyone's fresh values
// Every measure's values as they are now, without updating anything
pub fn current_values(measures: &[Box<dyn Measure>]) -> Values {
    measures.iter().map(|m| (m.name().to_lowercase(), sample_of(&**m))).collect()
}

pub fn update_all(measures: &mut [Box<dyn Measure>]) -> Values {
    // Start from each measure's current (old) value, so a Calc can read a measure defined below it
    let mut values = current_values(measures);
    // Then update in order: each fresh value replaces the old one for the measures after it
    for m in measures.iter_mut() {
        m.update_with(&values);
        values.insert(m.name().to_lowercase(), sample_of(&**m));
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    // A measure whose value goes up by 1 at each update
    struct Count(f64);

    impl Measure for Count {
        fn name(&self) -> &str {
            "Count"
        }
        fn update(&mut self) {
            self.0 += 1.0;
        }
        fn value(&self) -> f64 {
            self.0
        }
    }

    #[test]
    fn disabled_and_paused() {
        let mut m = Switch::new(Box::new(Count(0.0)), false, false);
        m.update();
        m.update();
        assert_eq!(m.value(), 2.0);

        // Paused: no updates, the last value stays
        m.switch_mut().unwrap().paused = true;
        m.update();
        assert_eq!(m.value(), 2.0);

        // Paused wins over disabled: still the last value
        m.switch_mut().unwrap().disabled = true;
        m.update();
        assert_eq!(m.value(), 2.0);

        // Disabled alone: 0 from the next update on, and the count stops
        m.switch_mut().unwrap().paused = false;
        m.update();
        assert_eq!(m.value(), 0.0);
        assert_eq!(m.text(), "0");

        // Enabled again: it carries on from where it stopped
        m.switch_mut().unwrap().disabled = false;
        m.update();
        assert_eq!(m.value(), 3.0);

        // !SetOption: Disabled and MaxValue are read again at the next update
        let section = Section { name: "Count".to_string(), options: HashMap::new() };
        let mut ranged = Ranged::wrap(Box::new(Count(0.0)));
        ranged.read_range(&section, &HashMap::new());
        let mut s = Switch::new(Box::new(ranged), false, false).with_source(&section, &HashMap::new());
        s.update();
        assert_eq!(s.range(), (0.0, 100.0)); // Count's own range
        s.switch_mut().unwrap().set_option("MaxValue", "#Top#");
        s.switch_mut().unwrap().set_variable("top", "50");
        s.switch_mut().unwrap().set_option("Disabled", "1");
        assert_eq!(s.value(), 1.0); // nothing changes before the update
        s.update();
        assert_eq!((s.value(), s.range()), (0.0, (0.0, 50.0)));
        s.switch_mut().unwrap().set_option("Disabled", ""); // removed: enabled again
        s.update();
        assert_eq!(s.value(), 2.0);

        // A bang writes the option too, so a later !SetOption doesn't undo it
        s.switch_mut().unwrap().set_disabled(true);
        s.switch_mut().unwrap().set_option("MinValue", "10");
        s.update();
        assert_eq!((s.value(), s.range()), (0.0, (10.0, 50.0)));

        // Disabled= at load: 0 after the first update
        let mut off = Switch::new(Box::new(Count(0.0)), true, false);
        off.update();
        assert_eq!(off.value(), 0.0);
    }
}
