//! Diagnosis runner for one execution cycle. Resolves a provider once, then
//! fetches a snapshot and evaluates the rule registry into a single
//! [`DiagnosisResult`] on each call. Rendering, persistence, output, and
//! scheduling belong to the caller.
use std::time::Duration;

use crate::cli::clients::ConnectionOptions;
use crate::cli::config::CliConfig;
use crate::cli::providers::resolve_provider;
use crate::core::config::CoreConfig;
use crate::core::diagnosis::diagnose;
use crate::core::models::{DiagnosisResult, TargetMetadata};
use crate::core::providers::{Provider, ProviderError};
use crate::core::rules::build_registry;

/// Input for a diagnosis run. Owned so it can be constructed once and reused
/// across watch iterations.
#[derive(Clone)]
pub struct DiagnoseRequest {
    pub url: String,
    pub since: String,
    pub model: Option<String>,
    pub timeout: f64,
    pub interval: Duration,
    pub config: CliConfig,
    pub conn_opts: ConnectionOptions,
    pub target: TargetMetadata,
}

/// Error from a diagnosis run.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("could not read metrics: {0}")]
    Fetch(#[from] ProviderError),
}

pub struct DiagnoseRunner {
    request: DiagnoseRequest,
    provider: Box<dyn Provider>,
}

impl DiagnoseRunner {
    /// Resolve the provider (HTTP probe) once. Subsequent `run_once` calls
    /// reuse the resolved provider without re-probing.
    pub async fn new(request: DiagnoseRequest) -> Result<Self, RunnerError> {
        let provider = resolve_provider(
            &request.url,
            request.timeout,
            &request.conn_opts,
            &request.since,
            request.model.as_deref(),
        )
        .await?;
        Ok(Self { request, provider })
    }

    pub fn config(&self) -> &CliConfig {
        &self.request.config
    }

    pub fn interval(&self) -> Duration {
        self.request.interval
    }

    /// Run one diagnosis cycle and return the raw result.
    /// Rendering, persistence, and output are the caller's responsibility.
    pub async fn run_once(&self) -> Result<DiagnosisResult, RunnerError> {
        let core_config = CoreConfig {
            rules: self.request.config.rules.clone(),
        };
        let registry = build_registry(&core_config);
        diagnose(
            self.provider.as_ref(),
            &registry,
            &self.request.since,
            self.request.model.as_deref(),
            &self.request.target,
            &core_config,
        )
        .await
        .map_err(RunnerError::from)
    }
}

pub fn firing_ids(result: &DiagnosisResult) -> Vec<String> {
    result
        .checks
        .iter()
        .filter(|c| c.finding.is_some())
        .map(|c| c.id.clone())
        .collect()
}

pub fn transition_label(prev: Option<&DiagnosisResult>, curr: &DiagnosisResult) -> Option<String> {
    match prev {
        None => Some("initial".to_string()),
        Some(prev) => {
            if prev.health() != curr.health() {
                Some(format!("{} → {}", prev.health(), curr.health()))
            } else if firing_ids(prev) != firing_ids(curr) {
                Some("rules changed".to_string())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::models::{Confidence, Finding, RuleResult, Severity};

    fn result_with(findings: Vec<Option<Finding>>) -> DiagnosisResult {
        let checks = findings
            .into_iter()
            .enumerate()
            .map(|(i, finding)| RuleResult {
                id: format!("rule-{i}"),
                name: format!("Rule {i}"),
                title: format!("Rule {i}"),
                severity: Severity::Warning,
                finding,
            })
            .collect();
        DiagnosisResult::new(
            crate::core::models::DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            checks,
        )
    }

    fn finding(severity: Severity) -> Finding {
        Finding {
            severity,
            confidence: Confidence::Medium,
            title: "Test".to_string(),
            signals: vec![],
            evidence: vec![],
            likely_causes: vec![],
            recommendations: vec![],
            related_metrics: vec![],
        }
    }

    #[test]
    fn firing_ids_extracts_only_firing_checks() {
        let result = result_with(vec![Some(finding(Severity::Warning)), None]);
        let ids = firing_ids(&result);
        assert_eq!(ids, vec!["rule-0".to_string()]);
    }

    #[test]
    fn transition_label_initial() {
        let curr = result_with(vec![Some(finding(Severity::Warning))]);
        assert_eq!(transition_label(None, &curr), Some("initial".to_string()));
    }

    #[test]
    fn transition_label_health_change() {
        let prev = result_with(vec![Some(finding(Severity::Warning))]);
        let curr = result_with(vec![Some(finding(Severity::Critical))]);
        assert_eq!(
            transition_label(Some(&prev), &curr),
            Some("warning → critical".to_string())
        );
    }

    #[test]
    fn transition_label_rules_changed() {
        let prev = result_with(vec![Some(finding(Severity::Warning)), None]);
        let curr = result_with(vec![
            Some(finding(Severity::Warning)),
            Some(finding(Severity::Warning)),
        ]);
        assert_eq!(
            transition_label(Some(&prev), &curr),
            Some("rules changed".to_string())
        );
    }

    #[test]
    fn transition_label_no_change() {
        let prev = result_with(vec![Some(finding(Severity::Warning))]);
        let curr = result_with(vec![Some(finding(Severity::Warning))]);
        assert_eq!(transition_label(Some(&prev), &curr), None);
    }

    #[test]
    fn runner_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RunnerError>();
    }
}
