//! Metric specifications: how raw probe series are turned into reportable metrics.
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use super::observations::ObservationSpec;
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
    fn observation_spec(&self) -> &ObservationSpec;

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
    pub observation: ObservationSpec,
}

impl Direct {
    pub fn new(
        output: impl Into<String>,
        probe: impl Into<String>,
        display: MetricDisplay,
        observation: ObservationSpec,
    ) -> Self {
        Self {
            output: output.into(),
            probe: probe.into(),
            display,
            aggregate_by: Aggregate::Sum,
            observation,
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

    fn observation_spec(&self) -> &ObservationSpec {
        &self.observation
    }
}

/// Aggregate ratio: `numerator.sum() / denominator.sum()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ratio {
    pub output: String,
    pub numerator: String,
    pub denominator: String,
    pub display: MetricDisplay,
    pub observation: ObservationSpec,
}

impl Ratio {
    pub fn new(
        output: impl Into<String>,
        numerator: impl Into<String>,
        denominator: impl Into<String>,
        display: MetricDisplay,
        observation: ObservationSpec,
    ) -> Self {
        Self {
            output: output.into(),
            numerator: numerator.into(),
            denominator: denominator.into(),
            display,
            observation,
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

    fn observation_spec(&self) -> &ObservationSpec {
        &self.observation
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
    use crate::metrics::{NO_DIMENSIONS, ObservationKind, ObservationRollup, ObservationUnit};

    const TEST_OBS: ObservationSpec = ObservationSpec {
        id: "test",
        unit: ObservationUnit::Count,
        kind: ObservationKind::Gauge,
        rollup: ObservationRollup::Sum,
        quantile: None,
        dimensions: NO_DIMENSIONS,
    };

    #[test]
    fn direct_probe_names_contain_output_probe() {
        let spec = Direct::new("x", "probe_x", MetricDisplay::new("X"), TEST_OBS);
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
            TEST_OBS,
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
            TEST_OBS,
        );
        let series = spec.compute(&raw);
        assert!(series.value().is_none());
    }

    #[test]
    fn metric_specs_by_output_lookup_works() {
        assert!(METRIC_SPECS_BY_OUTPUT.contains_key("num_requests_running"));
        assert!(METRIC_SPECS_BY_OUTPUT.contains_key("prefix_cache_hit_rate"));
    }

    /// Table-driven registry invariant test.
    ///
    /// Each row is the expected (id, unit, kind, rollup, quantile, has_replica)
    /// tuple for every collected observation, in registry order.  Verifying the
    /// full table in one pass keeps the stable external contract honest: any
    /// drift in the macro table shows up as a test failure here.
    #[test]
    fn registry_invariants_match_expected_table() {
        use crate::metrics::Signal;
        use crate::metrics::{ObservationDimension, REPLICA_DIMENSION};

        // (id, unit, kind, rollup, quantile, has_replica)
        type Row = (
            &'static str,
            ObservationUnit,
            ObservationKind,
            ObservationRollup,
            Option<f64>,
            bool,
        );
        let expected: [Row; 13] = [
            (
                "requests_running",
                ObservationUnit::Count,
                ObservationKind::Gauge,
                ObservationRollup::Sum,
                None,
                true,
            ),
            (
                "requests_waiting",
                ObservationUnit::Count,
                ObservationKind::Gauge,
                ObservationRollup::Sum,
                None,
                true,
            ),
            (
                "kv_cache_usage",
                ObservationUnit::Ratio,
                ObservationKind::Gauge,
                ObservationRollup::Max,
                None,
                true,
            ),
            (
                "prompt_token_throughput",
                ObservationUnit::TokensPerSecond,
                ObservationKind::Gauge,
                ObservationRollup::Sum,
                None,
                true,
            ),
            (
                "generation_token_throughput",
                ObservationUnit::TokensPerSecond,
                ObservationKind::Gauge,
                ObservationRollup::Sum,
                None,
                true,
            ),
            (
                "requests_succeeded",
                ObservationUnit::Count,
                ObservationKind::WindowDelta,
                ObservationRollup::Sum,
                None,
                false,
            ),
            (
                "requests_failed",
                ObservationUnit::Count,
                ObservationKind::WindowDelta,
                ObservationRollup::Sum,
                None,
                false,
            ),
            (
                "requests_aborted",
                ObservationUnit::Count,
                ObservationKind::WindowDelta,
                ObservationRollup::Sum,
                None,
                false,
            ),
            (
                "time_to_first_token",
                ObservationUnit::Seconds,
                ObservationKind::Quantile,
                ObservationRollup::Max,
                Some(0.95),
                false,
            ),
            (
                "time_per_output_token",
                ObservationUnit::Seconds,
                ObservationKind::Quantile,
                ObservationRollup::Max,
                Some(0.95),
                false,
            ),
            (
                "queue_time",
                ObservationUnit::Seconds,
                ObservationKind::Quantile,
                ObservationRollup::Max,
                Some(0.95),
                false,
            ),
            (
                "preemptions",
                ObservationUnit::Count,
                ObservationKind::WindowDelta,
                ObservationRollup::Sum,
                None,
                false,
            ),
            (
                "prefix_cache_hit_rate",
                ObservationUnit::Ratio,
                ObservationKind::WindowRatio,
                ObservationRollup::Ratio,
                None,
                false,
            ),
        ];

        let specs = all_specs();
        assert_eq!(
            specs.len(),
            expected.len(),
            "registry has {} specs, expected {}",
            specs.len(),
            expected.len()
        );

        let approved_gauges_with_replica: &[&str] = &[
            "requests_running",
            "requests_waiting",
            "kv_cache_usage",
            "prompt_token_throughput",
            "generation_token_throughput",
        ];

        let mut seen_ids: HashSet<&str> = HashSet::new();
        let mut computed_outputs: HashSet<String> = HashSet::new();
        // Computed signals are not in METRIC_SPECS; collect their names from the
        // Signal enum so we can assert they never appear as collected outputs.
        {
            let computed = [
                Signal::TotalRequests,
                Signal::ErrorRate,
                Signal::AbortRate,
                Signal::ReplicaRunningImbalance,
            ];
            for s in computed {
                computed_outputs.insert(s.to_string());
            }
        }

        for (i, spec) in specs.iter().enumerate() {
            let obs = spec.observation_spec();
            let out = spec.output();

            // (10) Computed signals do not appear in all_specs().
            assert!(
                !computed_outputs.contains(out),
                "computed signal `{out}` must not appear in all_specs()"
            );

            // (1) nonempty ASCII snake-case ID
            assert!(
                !obs.id.is_empty(),
                "spec #{i} (`{out}`) has empty observation id"
            );
            assert!(
                obs.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit()),
                "spec #{i} (`{out}`) observation id `{}` is not ASCII snake-case",
                obs.id
            );
            assert!(
                !obs.id.starts_with('_') && !obs.id.ends_with('_') && !obs.id.contains("__"),
                "spec #{i} (`{out}`) observation id `{}` has leading/trailing/double underscore",
                obs.id
            );

            // (2) uniqueness
            assert!(
                seen_ids.insert(obs.id),
                "duplicate observation id `{}`",
                obs.id
            );

            // (4) no vllm: prefix
            assert!(
                !obs.id.starts_with("vllm:"),
                "observation id `{}` starts with `vllm:`",
                obs.id
            );

            let has_replica = obs.dimensions.contains(&ObservationDimension::Replica);

            // (3) exact table match (in order)
            let (eid, eunit, ekind, erollup, equant, ereplica) = expected[i];
            assert_eq!(
                obs.id, eid,
                "spec #{i} id mismatch: got `{}`, want `{eid}`",
                obs.id
            );
            assert_eq!(obs.unit, eunit, "spec #{i} (`{}`) unit mismatch", obs.id);
            assert_eq!(obs.kind, ekind, "spec #{i} (`{}`) kind mismatch", obs.id);
            assert_eq!(
                obs.rollup, erollup,
                "spec #{i} (`{}`) rollup mismatch",
                obs.id
            );
            assert_eq!(
                obs.quantile, equant,
                "spec #{i} (`{}`) quantile mismatch",
                obs.id
            );
            assert_eq!(
                has_replica, ereplica,
                "spec #{i} (`{}`) has_replica mismatch",
                obs.id
            );

            // (5) WindowDelta => unit Count, no quantile
            if obs.kind == ObservationKind::WindowDelta {
                assert_eq!(
                    obs.unit,
                    ObservationUnit::Count,
                    "WindowDelta `{}` must have unit Count",
                    obs.id
                );
                assert!(
                    obs.quantile.is_none(),
                    "WindowDelta `{}` must have no quantile",
                    obs.id
                );
            }

            // (6) Quantile => finite value strictly in (0,1)
            if obs.kind == ObservationKind::Quantile {
                let q = obs
                    .quantile
                    .expect("Quantile observation must have a quantile value");
                assert!(q.is_finite(), "Quantile `{}` has non-finite value", obs.id);
                assert!(
                    (0.0..=1.0).contains(&q) && q != 0.0 && q != 1.0,
                    "Quantile `{}` value {q} not strictly in (0,1)",
                    obs.id
                );
            }

            // (7) only approved gauges may use Replica
            if has_replica {
                assert!(
                    obs.kind == ObservationKind::Gauge,
                    "observation `{}` uses Replica but is not a Gauge",
                    obs.id
                );
                assert!(
                    approved_gauges_with_replica.contains(&obs.id),
                    "observation `{}` uses Replica but is not in the approved gauge set",
                    obs.id
                );
            }

            // Ratio metrics use ratio-compatible units and rollup.
            if obs.kind == ObservationKind::WindowRatio {
                assert!(
                    matches!(obs.unit, ObservationUnit::Ratio),
                    "ratio `{}` must have unit Ratio",
                    obs.id
                );
                assert_eq!(
                    obs.rollup,
                    ObservationRollup::Ratio,
                    "ratio `{}` must have rollup Ratio",
                    obs.id
                );
            }
        }

        // (3) full ordered list match: ensure REPLICA_DIMENSION const still
        // has exactly one element (sanity for the dimension type contract).
        assert_eq!(REPLICA_DIMENSION.len(), 1);
    }
}
