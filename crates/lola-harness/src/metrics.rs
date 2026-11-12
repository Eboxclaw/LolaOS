//! Per-stage latency metrics for the decision loop (S1). Tiny by design —
//! no histogram dependency; samples are nanosecond durations per stage
//! name, reported as p50/p95/max.

use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StageMetricsSnapshot {
    pub samples: usize,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub max_ns: u64,
}

#[derive(Debug, Default)]
pub struct StageMetrics {
    stages: BTreeMap<&'static str, Vec<u64>>,
}

impl StageMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Time a stage and record it.
    pub fn measure<T>(&mut self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let value = f();
        let elapsed = start.elapsed().as_nanos() as u64;
        self.stages.entry(stage).or_default().push(elapsed);
        value
    }

    pub fn snapshot(&self, stage: &str) -> Option<StageMetricsSnapshot> {
        let samples = self.stages.get(stage)?;
        let mut sorted = samples.clone();
        sorted.sort_unstable();
        let at = |q: f64| sorted[((sorted.len() as f64 - 1.0) * q).round() as usize];
        Some(StageMetricsSnapshot {
            samples: sorted.len(),
            p50_ns: at(0.50),
            p95_ns: at(0.95),
            max_ns: sorted[sorted.len() - 1],
        })
    }

    pub fn stages(&self) -> Vec<&'static str> {
        self.stages.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_ordered_and_counted() {
        let mut metrics = StageMetrics::new();
        for i in 0..100 {
            metrics.measure("stage", || std::hint::black_box(i));
        }
        let snap = metrics.snapshot("stage").expect("recorded");
        assert_eq!(snap.samples, 100);
        assert!(snap.p50_ns <= snap.p95_ns);
        assert!(snap.p95_ns <= snap.max_ns);
        assert!(metrics.snapshot("missing").is_none());
    }
}
