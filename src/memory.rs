use crate::measure::Measure;
use std::{collections::HashMap, fs};

fn read_meminfo() -> HashMap<String, u64> {
    let text = fs::read_to_string("/proc/meminfo").expect("cannot read /proc/meminfo");
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
            if let Ok(kb) = val.parse::<u64>() {
                map.insert(key.trim_end_matches(':').to_string(), kb);
            }
        }
    }
    map
}

pub struct MemoryMeasure {
    name: String,
    used_kb: u64,
    total_kb: u64,
}

impl MemoryMeasure {
    pub fn new(name: &str) -> Self {
        let mut m = Self {
            name: name.to_string(),
            used_kb: 0,
            total_kb: 0,
        };
        m.update();
        m
    }
}

impl Measure for MemoryMeasure {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) {
        let mem = read_meminfo();
        self.total_kb = mem.get("MemTotal").copied().unwrap_or(0);
        let available = mem.get("MemAvailable").copied().unwrap_or(0);
        self.used_kb = self.total_kb.saturating_sub(available);
    }

    fn value(&self) -> f64 {
        if self.total_kb == 0 {
            0.0
        } else {
            100.0 * self.used_kb as f64 / self.total_kb as f64
        }
    }

    // Overrides the default text()
    fn text(&self) -> String {
        format!(
            "{:.0} / {:.0} MiB",
            self.used_kb as f64 / 1024.0,
            self.total_kb as f64 / 1024.0
        )
    }
}
