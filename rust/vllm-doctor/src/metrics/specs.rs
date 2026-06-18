//! Metric specifications: how raw probe series are turned into reportable metrics.
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use super::series::{Aggregate, MetricSeries};
use crate::metrics::MetricSeriesSnapshot;

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
        match self.output() {
            "num_requests_running" => snapshot.num_requests_running.value(),
            "num_requests_waiting" => snapshot.num_requests_waiting.value(),
            "kv_cache_usage_perc" => snapshot.kv_cache_usage_perc.value(),
            "prompt_tokens_per_second" => snapshot.prompt_tokens_per_second.value(),
            "generation_tokens_per_second" => snapshot.generation_tokens_per_second.value(),
            "request_success_total" => snapshot.request_success_total.value(),
            "request_error_total" => snapshot.request_error_total.value(),
            "request_abort_total" => snapshot.request_abort_total.value(),
            "ttft_p95_seconds" => snapshot.ttft_p95_seconds.value(),
            "tpot_p95_seconds" => snapshot.tpot_p95_seconds.value(),
            "prefix_cache_hit_rate" => snapshot.prefix_cache_hit_rate.value(),
            "queue_time_p95_seconds" => snapshot.queue_time_p95_seconds.value(),
            "num_preemptions_total" => snapshot.num_preemptions_total.value(),
            _ => None,
        }
    }

    /// Borrow the raw series backing this metric, for per-label breakdowns.
    fn series<'a>(&self, snapshot: &'a MetricSeriesSnapshot) -> Option<&'a MetricSeries> {
        Some(match self.output() {
            "num_requests_running" => &snapshot.num_requests_running,
            "num_requests_waiting" => &snapshot.num_requests_waiting,
            "kv_cache_usage_perc" => &snapshot.kv_cache_usage_perc,
            "prompt_tokens_per_second" => &snapshot.prompt_tokens_per_second,
            "generation_tokens_per_second" => &snapshot.generation_tokens_per_second,
            "request_success_total" => &snapshot.request_success_total,
            "request_error_total" => &snapshot.request_error_total,
            "request_abort_total" => &snapshot.request_abort_total,
            "ttft_p95_seconds" => &snapshot.ttft_p95_seconds,
            "tpot_p95_seconds" => &snapshot.tpot_p95_seconds,
            "prefix_cache_hit_rate" => &snapshot.prefix_cache_hit_rate,
            "queue_time_p95_seconds" => &snapshot.queue_time_p95_seconds,
            "num_preemptions_total" => &snapshot.num_preemptions_total,
            _ => return None,
        })
    }
}

/// Return all defined metric specs.
pub fn all_specs() -> &'static [Box<dyn MetricSpec + Send + Sync>] {
    &*METRIC_SPECS
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

pub static METRIC_SPECS: LazyLock<[Box<dyn MetricSpec + Send + Sync>; 13]> = LazyLock::new(|| {
    [
        Box::new(Direct::new(
            "num_requests_running",
            "num_requests_running",
            MetricDisplay::new("Requests Running"),
        )),
        Box::new(Direct::new(
            "num_requests_waiting",
            "num_requests_waiting",
            MetricDisplay::new("Requests Waiting"),
        )),
        Box::new(
            Direct::new(
                "kv_cache_usage_perc",
                "kv_cache_usage_perc",
                MetricDisplay::new("GPU Cache Usage")
                    .with_fmt(".0%")
                    .with_bar(),
            )
            .with_aggregate(Aggregate::Max),
        ),
        Box::new(Direct::new(
            "prompt_tokens_per_second",
            "prompt_tokens_per_second",
            MetricDisplay::new("Prefill Tokens/s").with_fmt(".1f"),
        )),
        Box::new(Direct::new(
            "generation_tokens_per_second",
            "generation_tokens_per_second",
            MetricDisplay::new("Decode Tokens/s").with_fmt(".1f"),
        )),
        Box::new(Direct::new(
            "request_success_total",
            "request_success_total",
            MetricDisplay::new("Requests Success"),
        )),
        Box::new(Direct::new(
            "request_error_total",
            "request_error_total",
            MetricDisplay::new("Requests Error"),
        )),
        Box::new(Direct::new(
            "request_abort_total",
            "request_abort_total",
            MetricDisplay::new("Requests Aborted"),
        )),
        Box::new(
            Direct::new(
                "ttft_p95_seconds",
                "ttft_p95_seconds",
                MetricDisplay::new("TTFT p95 (s)").with_fmt(".3f"),
            )
            .with_aggregate(Aggregate::Max),
        ),
        Box::new(
            Direct::new(
                "tpot_p95_seconds",
                "tpot_p95_seconds",
                MetricDisplay::new("TPOT p95 (s)").with_fmt(".3f"),
            )
            .with_aggregate(Aggregate::Max),
        ),
        Box::new(
            Direct::new(
                "queue_time_p95_seconds",
                "queue_time_p95_seconds",
                MetricDisplay::new("Queue Time p95 (s)").with_fmt(".3f"),
            )
            .with_aggregate(Aggregate::Max),
        ),
        Box::new(Direct::new(
            "num_preemptions_total",
            "num_preemptions_total",
            MetricDisplay::new("Preemptions Total"),
        )),
        Box::new(Ratio::new(
            "prefix_cache_hit_rate",
            "prefix_hits",
            "prefix_queries",
            MetricDisplay::new("Prefix Cache Hit Rate").with_fmt(".0%"),
        )),
    ]
});

pub static METRIC_SPECS_BY_OUTPUT: LazyLock<HashMap<&str, &(dyn MetricSpec + Send + Sync)>> =
    LazyLock::new(|| {
        METRIC_SPECS
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
