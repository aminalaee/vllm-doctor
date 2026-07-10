//! Report renderers for diagnostic results.
pub mod format;
pub mod json;
pub mod notices;
pub mod templates;
pub mod text;

use crate::metrics::MetricSeriesSnapshot;
use crate::models::RuleResult;
use crate::models::{Assessment, DiagnosisResult, Health};

/// How the text report should be rendered. The CLI fills these in from the
/// terminal (width, whether stdout is a TTY); tests and pipes use the defaults.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub verbose: bool,
    pub width: usize,
    pub color: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            verbose: false,
            width: 78,
            color: false,
        }
    }
}

/// A rendered or renderable diagnosis report.
#[must_use]
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

    pub fn assessment(&self) -> &Assessment {
        &self.diagnosis.assessment
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
        let config = crate::config::Config::default();
        let registry = build_registry(&config);
        let checks = registry.run_all(&snapshot, &config);
        let diagnosis = DiagnosisResult::new(DiagnosisContext::new("5m"), snapshot.clone(), checks);
        let report = Report::new(diagnosis);

        let text = crate::reports::text::render(&report, &RenderOptions::default());
        assert!(text.contains("Health:"));
        // Queue pressure fires; rules that cannot evaluate stay quiet (no panel).
        assert!(text.contains("Queue Pressure"));
        assert!(!text.contains("could not be evaluated"));

        let json = crate::reports::json::render(&report, false);
        assert_eq!(json["schema_version"], "1");
        assert!(!json["checks"].as_array().unwrap().is_empty());
    }
}
