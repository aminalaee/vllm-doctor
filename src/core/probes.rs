//! Probe definitions: data types for raw Prometheus queries.
//!
//! The `Probe` and `ProbeKind` types are pure data used by the metric registry.
//! The `run_probes` function lives in the CLI layer because it depends on the
//! HTTP `Client` trait.
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Gauge,
    Increase,
    Percentile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub kind: ProbeKind,
    pub metric: String,
    pub quantile: f64,
    pub labels: HashMap<String, String>,
}

impl Probe {
    pub(crate) fn new(kind: ProbeKind, metric: impl Into<String>) -> Self {
        Self {
            kind,
            metric: metric.into(),
            quantile: 0.0,
            labels: HashMap::new(),
        }
    }

    pub(crate) fn with_quantile(mut self, quantile: f64) -> Self {
        self.quantile = quantile;
        self
    }

    pub(crate) fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }
}
