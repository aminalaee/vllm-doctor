//! Report renderers for diagnostic results.
pub mod format;
pub mod json;
pub mod templates;
pub mod text;

use crate::metrics::MetricSeriesSnapshot;
use crate::models::RuleResult;
use crate::models::{DiagnosisResult, Health};

/// A rendered or renderable diagnosis report.
pub struct Report {
    pub diagnosis: DiagnosisResult,
}

impl Report {
    pub fn new(diagnosis: DiagnosisResult) -> Self {
        Self { diagnosis }
    }

    pub fn health(&self) -> Health {
        self.diagnosis.health()
    }

    pub fn checks(&self) -> &[RuleResult] {
        &self.diagnosis.checks
    }

    pub fn fired(&self) -> Vec<&RuleResult> {
        self.diagnosis
            .checks
            .iter()
            .filter(|check| check.finding.is_some())
            .collect()
    }

    pub fn metric_series(&self) -> &MetricSeriesSnapshot {
        &self.diagnosis.metric_series
    }

    pub fn since(&self) -> &str {
        &self.diagnosis.context.since
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisContext;
    use crate::rules::build_registry;

    fn pressure_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(2.0)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.45)]),
            ..Default::default()
        }
    }

    #[test]
    fn registry_to_report_to_renderers() {
        let snapshot = pressure_snapshot();
        let registry = build_registry(&crate::config::Config::default());
        let checks = registry.run_all(&snapshot);
        let diagnosis = DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            checks,
            metric_series: snapshot.clone(),
        };
        let report = Report::new(diagnosis);

        let text = crate::reports::text::render(&report, false);
        assert!(text.contains("Health:"));
        assert!(text.contains("Check"));

        let json = crate::reports::json::render(&report, false);
        assert_eq!(json["schema_version"], "1.0");
        assert!(!json["checks"].as_array().unwrap().is_empty());
    }
}
