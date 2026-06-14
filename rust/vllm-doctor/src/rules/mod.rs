//! Diagnostic rules.
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{FindingData, Severity};

pub mod kv_cache_pressure;
pub mod low_throughput;
pub mod preemption_pressure;
pub mod queue_latency;
pub mod queue_pressure;

pub trait Rule {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn likely_causes(&self) -> &'static [&'static str];
    fn recommendations(&self) -> &'static [&'static str];
    fn related_metrics(&self) -> &'static [&'static str];

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData>;
}
