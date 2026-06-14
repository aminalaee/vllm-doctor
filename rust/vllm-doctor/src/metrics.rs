//! Metrics primitives for the diagnostic engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricSeriesSnapshot {}

impl MetricSeriesSnapshot {
    pub fn to_metrics(&self) -> Metrics {
        Metrics::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_to_metrics_is_default() {
        let snapshot = MetricSeriesSnapshot::default();
        assert_eq!(snapshot.to_metrics(), Metrics::default());
    }
}
