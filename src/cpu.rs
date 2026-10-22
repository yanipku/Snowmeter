use crate::measure::Measure;
use std::fs;

fn read_cpu() -> (u64, u64) {
    let stat = fs::read_to_string("/proc/stat").expect("cannot read /proc/stat");
    let line = stat.lines().next().expect("/proc/stat is empty");
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    let idle = values[3] + values[4];
    let total: u64 = values.iter().take(8).sum();
    (idle, total)
}

pub struct CpuMeasure {
    name: String,
    prev_idle: u64,
    prev_total: u64,
    usage: f64,
}

impl CpuMeasure {
    pub fn new(name: &str) -> Self {
        let (idle, total) = read_cpu();
        Self {
            name: name.to_string(),
            prev_idle: idle,
            prev_total: total,
            usage: 0.0,
        }
    }
}

impl Measure for CpuMeasure {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) {
        let (idle, total) = read_cpu();
        let d_total = total.saturating_sub(self.prev_total);
        let d_idle = idle.saturating_sub(self.prev_idle);
        self.usage = if d_total > 0 {
            100.0 * d_total.saturating_sub(d_idle) as f64 / d_total as f64
        } else {
            0.0
        };
        self.prev_idle = idle;
        self.prev_total = total;
    }

    fn value(&self) -> f64 {
        self.usage
    }
}