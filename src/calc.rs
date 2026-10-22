use crate::formula::{self, Env, Expr, Suffix};
use crate::measure::{fraction_in, Measure, Values};
use std::time::{SystemTime, UNIX_EPOCH};

// A small xorshift random number generator: fine for Random in skins, not for security
struct Rng(u64);

impl Rng {
    fn new(seed_text: &str) -> Rng {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let mut h = nanos ^ 0x9E37_79B9_7F4A_7C15;
        for b in seed_text.bytes() {
            h = (h.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
        }
        Rng(if h == 0 { 0x2545_F491_4F6C_DD1D } else { h })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    // A whole number from low to high, both included
    fn between(&mut self, low: f64, high: f64) -> f64 {
        let (lo, hi) = if low <= high { (low, high) } else { (high, low) };
        let lo = lo.floor();
        let choices = ((hi.floor() - lo) + 1.0) as u64;
        lo + (self.next() % choices.max(1)) as f64
    }
}

// What a formula can see while a Calc measure updates
struct CalcEnv<'a> {
    values: &'a Values,
    counter: f64,
    random: f64,
}

impl Env for CalcEnv<'_> {
    fn get(&self, name: &str, suffix: Suffix) -> Option<f64> {
        if suffix == Suffix::Value {
            match name {
                "counter" => return Some(self.counter),
                "random" => return Some(self.random),
                _ => {}
            }
        }
        let s = self.values.get(name)?;
        Some(match suffix {
            Suffix::Value => s.value,
            Suffix::Percent => fraction_in(s.value, s.min, s.max) * 100.0,
            Suffix::Min => s.min,
            Suffix::Max => s.max,
        })
    }
}

pub struct CalcMeasure {
    name: String,
    expr: Option<Expr>, // None if the formula couldn't be read
    value: f64,
    low: f64,
    high: f64,
    update_random: bool,
    random: f64,
    rng: Rng,
    counter: u64,
    seen_min: f64, // Calc follows the smallest and largest values it has produced
    seen_max: f64,
    failing: bool, // so an error is reported once, not every update
}

impl CalcMeasure {
    pub fn new(name: &str, formula: &str, low: f64, high: f64, update_random: bool) -> CalcMeasure {
        let expr = match formula::parse(formula) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("[{}] cannot read Formula '{}': {}", name, formula, e);
                None
            }
        };
        let mut rng = Rng::new(name);
        let random = rng.between(low, high);
        CalcMeasure {
            name: name.to_string(),
            expr,
            value: 0.0,
            low,
            high,
            update_random,
            random,
            rng,
            counter: 0,
            seen_min: 0.0,
            seen_max: 1.0, // Rainmeter's default range before any value widens it
            failing: false,
        }
    }
}

impl Measure for CalcMeasure {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) {
        self.update_with(&Values::new());
    }

    fn update_with(&mut self, values: &Values) {
        self.counter += 1;
        if self.update_random {
            self.random = self.rng.between(self.low, self.high);
        }
        let result = match &self.expr {
            Some(expr) => expr.eval(&CalcEnv {
                values,
                counter: self.counter as f64,
                random: self.random,
            }),
            None => Ok(0.0),
        };
        match result {
            Ok(v) => {
                self.value = v;
                self.failing = false;
                self.seen_min = self.seen_min.min(v);
                self.seen_max = self.seen_max.max(v);
            }
            Err(e) => {
                if !self.failing {
                    eprintln!("[{}] formula error: {} (value set to 0)", self.name, e);
                    self.failing = true;
                }
                self.value = 0.0;
            }
        }
    }

    fn value(&self) -> f64 {
        self.value
    }

    fn range(&self) -> (f64, f64) {
        (self.seen_min, self.seen_max)
    }

    fn text(&self) -> String {
        format_number(self.value)
    }
}

// 5 -> "5", 3.14159265 -> "3.14159", 0.5 -> "0.5"
fn format_number(v: f64) -> String {
    let s = format!("{:.5}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}