//! Advisory notices about how to read a report.
//!
//! Notices are caveats about how to interpret the report — e.g. that scrape mode
//! omits latency rules, or that metrics blend across models when no `--model` is given.
use crate::metrics::{MODEL_LABEL, label_values};
use crate::models::{ClientMode, DiagnosisResult};

pub const SCRAPE_MODE_NOTICE: &str = "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis.";
pub const MULTI_MODEL_NOTICE: &str = "Multiple models found in this target — metrics are aggregated across all of them. Pass --model to scope the report to one.";

/// All mode/data notices that apply to a report.
pub fn resolve_notices(result: &DiagnosisResult) -> Vec<String> {
    let mut notices = Vec::new();
    if result.context.client_mode == ClientMode::Scrape {
        notices.push(SCRAPE_MODE_NOTICE.to_string());
    }
    if result.context.model_name.is_none()
        && label_values(&result.metric_series, MODEL_LABEL).len() > 1
    {
        notices.push(MULTI_MODEL_NOTICE.to_string());
    }
    notices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::{ClientMode, DiagnosisContext, DiagnosisResult};

    fn result(
        mode: ClientMode,
        model: Option<&str>,
        series: MetricSeriesSnapshot,
    ) -> DiagnosisResult {
        let mut context = DiagnosisContext::new("5m").with_client_mode(mode);
        if let Some(model) = model {
            context = context.with_model_name(model);
        }
        DiagnosisResult::new(context, series, vec![])
    }

    fn multi_model_series() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                MetricSample::new(1.0).with_label(MODEL_LABEL, "a"),
                MetricSample::new(2.0).with_label(MODEL_LABEL, "b"),
            ]),
            ..Default::default()
        }
    }

    #[test]
    fn scrape_mode_emits_latency_notice() {
        let notices = resolve_notices(&result(
            ClientMode::Scrape,
            None,
            MetricSeriesSnapshot::default(),
        ));
        assert!(notices.iter().any(|n| n == SCRAPE_MODE_NOTICE));
    }

    #[test]
    fn multi_model_emits_notice_when_unscoped() {
        let notices = resolve_notices(&result(ClientMode::Prometheus, None, multi_model_series()));
        assert!(notices.iter().any(|n| n == MULTI_MODEL_NOTICE));
    }

    #[test]
    fn multi_model_suppressed_when_scoped() {
        let notices = resolve_notices(&result(
            ClientMode::Prometheus,
            Some("a"),
            multi_model_series(),
        ));
        assert!(!notices.iter().any(|n| n == MULTI_MODEL_NOTICE));
    }

    #[test]
    fn no_notices_for_clean_prometheus_single_model() {
        let notices = resolve_notices(&result(
            ClientMode::Prometheus,
            None,
            MetricSeriesSnapshot::default(),
        ));
        assert!(notices.is_empty());
    }
}
