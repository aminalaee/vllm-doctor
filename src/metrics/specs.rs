//! Metric specifications: how raw probe series are turned into reportable metrics.
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use super::series::{Aggregate, MetricSeries};
use crate::metrics::{MetricSeriesSnapshot, extract_by_output, series_by_output};

#[derive(Debug, Clone, PartialEq)]
pub struct MetricDisplay {
    pub title: String,
    pub fmt: String,
    pub bar: bool,
}

impl MetricDisplay {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fmt: ".0f".into(),
            bar: false,
        }
    }

    pub fn with_fmt(mut self, fmt: impl Into<String>) -> Self {
        self.fmt = fmt.into();
        self
    }

    pub fn with_bar(mut self) -> Self {
        self.bar = true;
        self
    }
}

pub trait MetricSpec: std::fmt::Debug + Send + Sync {
    fn output(&self) -> &str;
    fn display(&self) -> &MetricDisplay;
    fn probe_names(&self) -> HashSet<String>;
    fn compute(&self, raw: &HashMap<String, MetricSeries>) -> MetricSeries;

    /// Extract the aggregated value for this metric from a snapshot.
    fn extract(&self, snapshot: &MetricSeriesSnapshot) -> Option<f64> {
        extract_by_output(self.output(), snapshot)
    }

    /// Borrow the raw series backing this metric, for per-label breakdowns.
    fn series<'a>(&self, snapshot: &'a MetricSeriesSnapshot) -> Option<&'a MetricSeries> {
        series_by_output(self.output(), snapshot)
    }
}

/// Return all defined metric specs.
pub fn all_specs() -> &'static [Box<dyn MetricSpec + Send + Sync>] {
    &*super::METRIC_SPECS
}

#[derive(Debug, Clone, PartialEq)]
pub struct Direct {
    pub output: String,
    pub probe: String,
    pub display: MetricDisplay,
    pub aggregate_by: Aggregate,
}

impl Direct {
    pub fn new(
        output: impl Into<String>,
        probe: impl Into<String>,
        display: MetricDisplay,
    ) -> Self {
        Self {
            output: output.into(),
            probe: probe.into(),
            display,
            aggregate_by: Aggregate::Sum,
        }
    }

    pub fn with_aggregate(mut self, aggregate_by: Aggregate) -> Self {
        self.aggregate_by = aggregate_by;
        self
    }
}

impl MetricSpec for Direct {
    fn output(&self) -> &str {
        &self.output
    }

    fn display(&self) -> &MetricDisplay {
        &self.display
    }

    fn probe_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        names.insert(self.probe.clone());
        names
    }

    fn compute(&self, raw: &HashMap<String, MetricSeries>) -> MetricSeries {
        let mut series = raw.get(&self.probe).cloned().unwrap_or_default();
        series.aggregate_by = self.aggregate_by;
        series
    }
}

/// Aggregate ratio: `numerator.sum() / denominator.sum()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ratio {
    pub output: String,
    pub numerator: String,
    pub denominator: String,
    pub display: MetricDisplay,
}

impl Ratio {
    pub fn new(
        output: impl Into<String>,
        numerator: impl Into<String>,
        denominator: impl Into<String>,
        display: MetricDisplay,
    ) -> Self {
        Self {
            output: output.into(),
            numerator: numerator.into(),
            denominator: denominator.into(),
            display,
        }
    }
}

impl MetricSpec for Ratio {
    fn output(&self) -> &str {
        &self.output
    }

    fn display(&self) -> &MetricDisplay {
        &self.display
    }

    fn probe_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        names.insert(self.numerator.clone());
        names.insert(self.denominator.clone());
        names
    }

    fn compute(&self, raw: &HashMap<String, MetricSeries>) -> MetricSeries {
        let numerator = raw
            .get(&self.numerator)
            .and_then(|s| s.aggregate_by.apply(&s.samples));
        let denominator = raw
            .get(&self.denominator)
            .and_then(|s| s.aggregate_by.apply(&s.samples));
        match (numerator, denominator) {
            (Some(n), Some(d)) if d > 0.0 => MetricSeries::scalar(n / d),
            _ => MetricSeries::empty(),
        }
    }
}

pub static METRIC_SPECS_BY_OUTPUT: LazyLock<HashMap<&str, &(dyn MetricSpec + Send + Sync)>> =
    LazyLock::new(|| {
        super::METRIC_SPECS
            .iter()
            .map(|spec| (spec.output(), &**spec))
            .collect()
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_probe_names_contain_output_probe() {
        let spec = Direct::new("x", "probe_x", MetricDisplay::new("X"));
        assert_eq!(spec.probe_names(), HashSet::from(["probe_x".to_string()]));
    }

    #[test]
    fn ratio_computes_from_raw() {
        let mut raw = HashMap::new();
        raw.insert("hits".to_string(), MetricSeries::scalar(80.0));
        raw.insert("queries".to_string(), MetricSeries::scalar(100.0));

        let spec = Ratio::new(
            "hit_rate",
            "hits",
            "queries",
            MetricDisplay::new("Hit Rate"),
        );
        let series = spec.compute(&raw);
        assert_eq!(series.value(), Some(0.8));
    }

    #[test]
    fn ratio_returns_empty_when_denominator_is_zero() {
        let mut raw = HashMap::new();
        raw.insert("hits".to_string(), MetricSeries::scalar(80.0));
        raw.insert("queries".to_string(), MetricSeries::scalar(0.0));

        let spec = Ratio::new(
            "hit_rate",
            "hits",
            "queries",
            MetricDisplay::new("Hit Rate"),
        );
        let series = spec.compute(&raw);
        assert!(series.value().is_none());
    }

    #[test]
    fn metric_specs_by_output_lookup_works() {
        assert!(METRIC_SPECS_BY_OUTPUT.contains_key("num_requests_running"));
        assert!(METRIC_SPECS_BY_OUTPUT.contains_key("prefix_cache_hit_rate"));
    }
}
