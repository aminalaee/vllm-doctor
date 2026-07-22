//! Metrics primitives for the diagnostic engine.
//!
//! All metric definitions — probe wiring, specs, snapshot fields, and signals —
//! are generated from a single `define_metrics!` invocation below.  Adding a new
//! metric is a one-line change in that table.
use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

pub mod observations;
pub mod series;
pub mod specs;
pub use specs::all_specs;

pub use observations::{
    NO_DIMENSIONS, ObservationDimension, ObservationKind, ObservationRollup, ObservationSpec,
    ObservationUnit, REPLICA_DIMENSION,
};
pub use series::{Aggregate, MetricSample, MetricSeries};
pub use specs::{Direct, METRIC_SPECS_BY_OUTPUT, MetricDisplay, MetricSpec, Ratio};

use crate::probes::{Probe, ProbeKind};

// ---------------------------------------------------------------------------
// define_metrics! — single source of truth for all metric definitions.
// ---------------------------------------------------------------------------
//
// The macro accepts three sections:
//   direct   — one probe → one snapshot field → one Direct spec → one Signal
//   ratio    — two probes → one snapshot field → one Ratio spec → one Signal
//   computed — signals with no backing field (derived in SignalGraph::evaluate)
//
// From this table the macro generates:
//   * `Signal` enum + `Display` impl  (re-exported by signals.rs)
//   * `MetricSeriesSnapshot` struct + `from_raw` + `fields`
//   * `PROBES` static                   (re-exported by probes.rs)
//   * `METRIC_SPECS` static             (re-exported by specs.rs)
//   * `evaluate_direct` helper           (used by SignalGraph::evaluate)
//   * `extract_by_output` / `series_by_output` helpers (used by MetricSpec)
//
// Adding a metric means adding ONE entry to the table below.
//
// Each `direct`/`ratio` entry uses an `:ident` for the Rust field/variant name
// and `stringify!` produces the matching string literal for probe names, spec
// outputs, and `Display` formatting.

/// Recursive token-count helper usable in `const` / array-size contexts.
macro_rules! count {
    () => { 0usize };
    ($_h:tt $($t:tt)*) => { 1usize + count!($($t)*) };
}

macro_rules! define_metrics {
    (
        direct {
            $(
                $d_field:ident, $d_signal:ident,
                probe: $d_probe_kind:ident, $d_probe_metric:literal,
                labels: $d_probe_labels:expr,
                quantile: $d_probe_quantile:expr,
                aggregate: Aggregate::$d_agg:ident,
                observation: $d_obs_id:literal, $d_obs_unit:ident, $d_obs_kind:ident, $d_obs_quantile:expr, $d_obs_dims:expr,
                display: $d_title:literal, $d_fmt:literal, $d_bar:expr
            );* $(;)?
        }
        ratio {
            $(
                $r_field:ident, $r_signal:ident,
                probes: [
                    ($p1_name:literal, $p1_kind:ident, $p1_metric:literal, $p1_labels:expr, $p1_quantile:expr),
                    ($p2_name:literal, $p2_kind:ident, $p2_metric:literal, $p2_labels:expr, $p2_quantile:expr)
                ],
                observation: $r_obs_id:literal, $r_obs_unit:ident, $r_obs_kind:ident, $r_obs_quantile:expr, $r_obs_dims:expr,
                display: $r_title:literal, $r_fmt:literal, $r_bar:expr
            );* $(;)?
        }
        computed {
            $( $c_signal:ident, $c_name:literal );* $(;)?
        }
    ) => {
        // -- Signal enum ------------------------------------------------------
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Signal {
            $($d_signal,)*
            $($r_signal,)*
            $($c_signal,)*
        }

        impl std::fmt::Display for Signal {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let s = match self {
                    $(Signal::$d_signal => stringify!($d_field),)*
                    $(Signal::$r_signal => stringify!($r_field),)*
                    $(Signal::$c_signal => $c_name,)*
                };
                write!(f, "{s}")
            }
        }

        // -- MetricSeriesSnapshot struct -------------------------------------
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        pub struct MetricSeriesSnapshot {
            $(
                pub $d_field: MetricSeries,
            )*
            $(
                pub $r_field: MetricSeries,
            )*
        }

        impl MetricSeriesSnapshot {
            pub fn from_raw(raw: std::collections::HashMap<String, MetricSeries>) -> Self {
                let mut snapshot = Self::default();
                for spec in METRIC_SPECS.iter() {
                    let series = spec.compute(&raw);
                    match spec.output() {
                        $(stringify!($d_field) => snapshot.$d_field = series,)*
                        $(stringify!($r_field) => snapshot.$r_field = series,)*
                        _ => {}
                    }
                }
                snapshot
            }

            pub(crate) fn fields(&self) -> [&MetricSeries; count!($($d_field)* $($r_field)*)] {
                [
                    $(&self.$d_field,)*
                    $(&self.$r_field,)*
                ]
            }
        }

        // -- evaluate_direct --------------------------------------------------
        pub(crate) fn evaluate_direct(signal: Signal, snapshot: &MetricSeriesSnapshot) -> Option<f64> {
            match signal {
                $(Signal::$d_signal => snapshot.$d_field.value(),)*
                $(Signal::$r_signal => snapshot.$r_field.value(),)*
                _ => None,
            }
        }

        // -- extract_by_output / series_by_output -----------------------------
        pub(crate) fn extract_by_output(output: &str, snapshot: &MetricSeriesSnapshot) -> Option<f64> {
            match output {
                $(stringify!($d_field) => snapshot.$d_field.value(),)*
                $(stringify!($r_field) => snapshot.$r_field.value(),)*
                _ => None,
            }
        }

        pub(crate) fn series_by_output<'a>(
            output: &str,
            snapshot: &'a MetricSeriesSnapshot,
        ) -> Option<&'a MetricSeries> {
            match output {
                $(stringify!($d_field) => Some(&snapshot.$d_field),)*
                $(stringify!($r_field) => Some(&snapshot.$r_field),)*
                _ => None,
            }
        }

        // -- PROBES static ----------------------------------------------------
        pub static PROBES: LazyLock<[(&str, Probe); count!($($d_field)* $($p1_name)* $($p2_name)*)]> =
            LazyLock::new(|| {
                [
                    $((
                        stringify!($d_field),
                        Probe::new(ProbeKind::$d_probe_kind, $d_probe_metric)
                            .with_quantile($d_probe_quantile)
                            .with_labels($d_probe_labels),
                    ),)*
                    $((
                        $p1_name,
                        Probe::new(ProbeKind::$p1_kind, $p1_metric)
                            .with_quantile($p1_quantile)
                            .with_labels($p1_labels),
                    ),)*
                    $((
                        $p2_name,
                        Probe::new(ProbeKind::$p2_kind, $p2_metric)
                            .with_quantile($p2_quantile)
                            .with_labels($p2_labels),
                    ),)*
                ]
            });

        // -- METRIC_SPECS static ---------------------------------------------
        pub static METRIC_SPECS: LazyLock<
            [Box<dyn MetricSpec + Send + Sync>; count!($($d_field)* $($r_field)*)],
        > = LazyLock::new(|| {
            [
                $(Box::new({
                    let mut d = Direct::new(
                        stringify!($d_field),
                        stringify!($d_field),
                        MetricDisplay::new($d_title).with_fmt($d_fmt),
                        ObservationSpec {
                            id: $d_obs_id,
                            unit: ObservationUnit::$d_obs_unit,
                            kind: ObservationKind::$d_obs_kind,
                            rollup: ObservationRollup::$d_agg,
                            quantile: $d_obs_quantile,
                            dimensions: $d_obs_dims,
                        },
                    );
                    if $d_bar {
                        d.display = d.display.with_bar();
                    }
                    d.with_aggregate(Aggregate::$d_agg)
                }) as Box<dyn MetricSpec + Send + Sync>,)*
                $(Box::new({
                    let mut r = Ratio::new(
                        stringify!($r_field),
                        $p1_name,
                        $p2_name,
                        MetricDisplay::new($r_title).with_fmt($r_fmt),
                        ObservationSpec {
                            id: $r_obs_id,
                            unit: ObservationUnit::$r_obs_unit,
                            kind: ObservationKind::$r_obs_kind,
                            rollup: ObservationRollup::Ratio,
                            quantile: $r_obs_quantile,
                            dimensions: $r_obs_dims,
                        },
                    );
                    if $r_bar {
                        r.display = r.display.with_bar();
                    }
                    r
                }) as Box<dyn MetricSpec + Send + Sync>,)*
            ]
        });
    };
}

define_metrics! {
    direct {
        num_requests_running, NumRequestsRunning,
        probe: Gauge, "vllm:num_requests_running",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "requests_running", Count, Gauge, None, REPLICA_DIMENSION,
        display: "Requests Running", ".0f", false;

        num_requests_waiting, NumRequestsWaiting,
        probe: Gauge, "vllm:num_requests_waiting",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "requests_waiting", Count, Gauge, None, REPLICA_DIMENSION,
        display: "Requests Waiting", ".0f", false;

        kv_cache_usage_perc, KvCacheUsagePerc,
        probe: Gauge, "vllm:kv_cache_usage_perc",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Max,
        observation: "kv_cache_usage", Ratio, Gauge, None, REPLICA_DIMENSION,
        display: "GPU Cache Usage", ".0%", true;

        prompt_tokens_per_second, PromptTokensPerSecond,
        probe: Gauge, "vllm:prompt_tokens_per_second",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "prompt_token_throughput", TokensPerSecond, Gauge, None, REPLICA_DIMENSION,
        display: "Prefill Tokens/s", ".1f", false;

        generation_tokens_per_second, GenerationTokensPerSecond,
        probe: Gauge, "vllm:generation_tokens_per_second",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "generation_token_throughput", TokensPerSecond, Gauge, None, REPLICA_DIMENSION,
        display: "Decode Tokens/s", ".1f", false;

        request_success_total, RequestSuccessTotal,
        probe: Increase, "vllm:request_success_total",
        labels: std::collections::HashMap::from([
            ("finished_reason".to_string(), "stop".to_string()),
        ]),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "requests_succeeded", Count, WindowDelta, None, NO_DIMENSIONS,
        display: "Requests Success", ".0f", false;

        request_error_total, RequestErrorTotal,
        probe: Increase, "vllm:request_success_total",
        labels: std::collections::HashMap::from([
            ("finished_reason".to_string(), "error".to_string()),
        ]),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "requests_failed", Count, WindowDelta, None, NO_DIMENSIONS,
        display: "Requests Error", ".0f", false;

        request_abort_total, RequestAbortTotal,
        probe: Increase, "vllm:request_success_total",
        labels: std::collections::HashMap::from([
            ("finished_reason".to_string(), "abort".to_string()),
        ]),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "requests_aborted", Count, WindowDelta, None, NO_DIMENSIONS,
        display: "Requests Aborted", ".0f", false;

        ttft_p95_seconds, TtftP95Seconds,
        probe: Percentile, "vllm:time_to_first_token_seconds",
        labels: std::collections::HashMap::new(),
        quantile: 0.95,
        aggregate: Aggregate::Max,
        observation: "time_to_first_token", Seconds, Quantile, Some(0.95), NO_DIMENSIONS,
        display: "TTFT p95 (s)", ".3f", false;

        tpot_p95_seconds, TpotP95Seconds,
        probe: Percentile, "vllm:request_time_per_output_token_seconds",
        labels: std::collections::HashMap::new(),
        quantile: 0.95,
        aggregate: Aggregate::Max,
        observation: "time_per_output_token", Seconds, Quantile, Some(0.95), NO_DIMENSIONS,
        display: "TPOT p95 (s)", ".3f", false;

        queue_time_p95_seconds, QueueTimeP95Seconds,
        probe: Percentile, "vllm:request_queue_time_seconds",
        labels: std::collections::HashMap::new(),
        quantile: 0.95,
        aggregate: Aggregate::Max,
        observation: "queue_time", Seconds, Quantile, Some(0.95), NO_DIMENSIONS,
        display: "Queue Time p95 (s)", ".3f", false;

        num_preemptions_total, NumPreemptionsTotal,
        probe: Increase, "vllm:num_preemptions_total",
        labels: std::collections::HashMap::new(),
        quantile: 0.0,
        aggregate: Aggregate::Sum,
        observation: "preemptions", Count, WindowDelta, None, NO_DIMENSIONS,
        display: "Preemptions Total", ".0f", false;
    }
    ratio {
        prefix_cache_hit_rate, PrefixCacheHitRate,
        probes: [
            ("prefix_hits", Increase, "vllm:prefix_cache_hits_total", std::collections::HashMap::new(), 0.0),
            ("prefix_queries", Increase, "vllm:prefix_cache_queries_total", std::collections::HashMap::new(), 0.0)
        ],
        observation: "prefix_cache_hit_rate", Ratio, WindowRatio, None, NO_DIMENSIONS,
        display: "Prefix Cache Hit Rate", ".0%", false;
    }
    computed {
        TotalRequests, "total_requests";
        ErrorRate, "error_rate";
        AbortRate, "abort_rate";
        ReplicaRunningImbalance, "replica_running_imbalance";
    }
}

pub const REPLICA_LABELS: [&str; 8] = [
    "pod",
    "pod_name",
    "kubernetes_pod_name",
    "instance",
    "host",
    "hostname",
    "server",
    "endpoint",
];

pub const MODEL_LABEL: &str = "model_name";

/// All distinct values of `label` across every sample in the snapshot.
pub fn label_values(snapshot: &MetricSeriesSnapshot, label: &str) -> HashSet<String> {
    let mut values = HashSet::new();
    for series in snapshot.fields() {
        for sample in &series.samples {
            if let Some(value) = sample.labels.get(label) {
                values.insert(value.clone());
            }
        }
    }
    values
}

/// Pick the first known label that has >1 distinct values across the metric series.
///
/// Returns `None` when the snapshot looks like a single-replica deployment.
pub fn detect_replica_label(snapshot: &MetricSeriesSnapshot) -> Option<&str> {
    REPLICA_LABELS
        .into_iter()
        .find(|&label| label_values(snapshot, label).len() > 1)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn sample(value: f64, labels: &[(&str, &str)]) -> MetricSample {
        MetricSample {
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            value,
            timestamp: None,
        }
    }

    #[test]
    fn snapshot_from_raw_applies_aggregation_specs() {
        let mut raw = HashMap::new();
        raw.insert(
            "kv_cache_usage_perc".to_string(),
            MetricSeries {
                samples: vec![sample(0.5, &[("pod", "a")]), sample(0.9, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
        );
        raw.insert("prefix_hits".to_string(), MetricSeries::scalar(80.0));
        raw.insert("prefix_queries".to_string(), MetricSeries::scalar(100.0));

        let snapshot = MetricSeriesSnapshot::from_raw(raw);
        assert_eq!(snapshot.kv_cache_usage_perc.value(), Some(0.9));
        assert_eq!(snapshot.prefix_cache_hit_rate.value(), Some(0.8));
    }

    #[test]
    fn label_values_collects_across_series() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
            num_requests_waiting: MetricSeries {
                samples: vec![sample(3.0, &[("pod", "c")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };

        let values = label_values(&snapshot, "pod");
        assert_eq!(values.len(), 3);
        assert!(values.contains("a"));
        assert!(values.contains("b"));
        assert!(values.contains("c"));
    }

    #[test]
    fn detect_replica_label_finds_pod() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };
        assert_eq!(detect_replica_label(&snapshot), Some("pod"));
    }

    #[test]
    fn detect_replica_label_returns_none_for_single_replica() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };
        assert_eq!(detect_replica_label(&snapshot), None);
    }
}
