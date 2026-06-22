//! Domain models shared across the diagnostic engine.
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::metrics::{MetricSeriesSnapshot, Metrics};
use crate::signals::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientMode {
    #[default]
    Prometheus,
    Scrape,
}

impl fmt::Display for ClientMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prometheus => write!(f, "prometheus"),
            Self::Scrape => write!(f, "scrape"),
        }
    }
}

impl FromStr for ClientMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prometheus" => Ok(Self::Prometheus),
            "scrape" => Ok(Self::Scrape),
            _ => Err(format!("unknown client mode: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
        }
    }
}

impl FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "critical" => Ok(Self::Critical),
            "warning" => Ok(Self::Warning),
            "info" => Ok(Self::Info),
            _ => Err(format!("unknown severity: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Health {
    Ok,
    Info,
    Warning,
    Critical,
}

impl fmt::Display for Health {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => write!(f, "healthy"),
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl FromStr for Health {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "healthy" => Ok(Self::Ok),
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "critical" => Ok(Self::Critical),
            _ => Err(format!("unknown health: {s}")),
        }
    }
}

impl From<Severity> for Health {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Info => Self::Info,
            Severity::Warning => Self::Warning,
            Severity::Critical => Self::Critical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

impl FromStr for Confidence {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            _ => Err(format!("unknown confidence: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosisState {
    Healthy,
    Firing(Judgment),
    Unknown(String),
}

/// A rule's verdict when it fires: how serious, how sure, and the driving signal.
/// The rule owns both severity and confidence; the report layer just presents them.
#[derive(Debug, Clone, PartialEq)]
pub struct Judgment {
    pub severity: Severity,
    pub confidence: Confidence,
    pub signal: Signal,
    pub value: f64,
}

impl DiagnosisState {
    pub fn firing(severity: Severity, confidence: Confidence, signal: Signal, value: f64) -> Self {
        Self::Firing(Judgment {
            severity,
            confidence,
            signal,
            value,
        })
    }

    pub fn unknown_signal(signal: Signal) -> Self {
        Self::Unknown(format!("{signal} signal is missing or non-finite"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisContext {
    pub since: String,
    pub model_name: Option<String>,
    pub client_mode: ClientMode,
}

impl DiagnosisContext {
    pub fn new(since: impl Into<String>) -> Self {
        Self {
            since: since.into(),
            model_name: None,
            client_mode: ClientMode::default(),
        }
    }

    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    pub fn with_client_mode(mut self, mode: ClientMode) -> Self {
        self.client_mode = mode;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FindingData {
    pub confidence: Confidence,
    pub summary: String,
    pub signals: Vec<String>,
    pub evidence: Vec<String>,
    pub severity: Option<Severity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub confidence: Confidence,
    pub title: String,
    pub summary: String,
    pub signals: Vec<String>,
    pub evidence: Vec<String>,
    pub likely_causes: Vec<String>,
    pub recommendations: Vec<String>,
    pub related_metrics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleResult {
    pub id: String,
    pub name: String,
    pub title: String,
    pub severity: Severity,
    pub finding: Option<Finding>,
}

impl RuleResult {
    pub fn is_significant(&self) -> bool {
        self.finding.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisResult {
    pub context: DiagnosisContext,
    pub metric_series: MetricSeriesSnapshot,
    pub checks: Vec<RuleResult>,
}

impl DiagnosisResult {
    pub fn metrics(&self) -> Metrics {
        self.metric_series.to_metrics()
    }

    pub fn health(&self) -> Health {
        self.checks
            .iter()
            .filter_map(|check| check.finding.as_ref().map(|f| f.severity))
            .min()
            .map_or(Health::Ok, Health::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_mode_roundtrip() {
        for mode in [ClientMode::Prometheus, ClientMode::Scrape] {
            let text = mode.to_string();
            assert_eq!(ClientMode::from_str(&text).unwrap(), mode);
        }
    }

    #[test]
    fn health_display_roundtrips_through_from_str() {
        for health in [Health::Ok, Health::Info, Health::Warning, Health::Critical] {
            let text = health.to_string();
            assert_eq!(Health::from_str(&text).unwrap(), health);
        }
        // `ok` is not a valid token — the Display form is `healthy`.
        assert!(Health::from_str("ok").is_err());
    }

    #[test]
    fn severity_ordering_worst_first() {
        assert!(Severity::Critical < Severity::Warning);
        assert!(Severity::Warning < Severity::Info);
    }

    #[test]
    fn severity_roundtrip() {
        for sev in [Severity::Critical, Severity::Warning, Severity::Info] {
            let text = sev.to_string();
            assert_eq!(Severity::from_str(&text).unwrap(), sev);
        }
    }

    #[test]
    fn health_from_severity() {
        assert_eq!(Health::from(Severity::Info), Health::Info);
        assert_eq!(Health::from(Severity::Warning), Health::Warning);
        assert_eq!(Health::from(Severity::Critical), Health::Critical);
    }

    #[test]
    fn diagnosis_health_is_ok_when_no_findings() {
        let result = DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            metric_series: MetricSeriesSnapshot::default(),
            checks: vec![],
        };
        assert_eq!(result.health(), Health::Ok);
    }

    #[test]
    fn diagnosis_health_rolls_up_worst_finding() {
        let result = DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            metric_series: MetricSeriesSnapshot::default(),
            checks: vec![
                RuleResult {
                    id: "info-rule".into(),
                    name: "Info Rule".into(),
                    title: "Info".into(),
                    severity: Severity::Info,
                    finding: Some(Finding {
                        severity: Severity::Info,
                        confidence: Confidence::Medium,
                        title: "Info".into(),
                        summary: "...".into(),
                        signals: vec![],
                        evidence: vec![],
                        likely_causes: vec![],
                        recommendations: vec![],
                        related_metrics: vec![],
                    }),
                },
                RuleResult {
                    id: "critical-rule".into(),
                    name: "Critical Rule".into(),
                    title: "Critical".into(),
                    severity: Severity::Critical,
                    finding: Some(Finding {
                        severity: Severity::Critical,
                        confidence: Confidence::High,
                        title: "Critical".into(),
                        summary: "...".into(),
                        signals: vec![],
                        evidence: vec![],
                        likely_causes: vec![],
                        recommendations: vec![],
                        related_metrics: vec![],
                    }),
                },
            ],
        };
        assert_eq!(result.health(), Health::Critical);
    }

    #[test]
    fn diagnosis_health_ignores_none_findings() {
        let result = DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            metric_series: MetricSeriesSnapshot::default(),
            checks: vec![RuleResult {
                id: "quiet".into(),
                name: "Quiet".into(),
                title: "Quiet".into(),
                severity: Severity::Info,
                finding: None,
            }],
        };
        assert_eq!(result.health(), Health::Ok);
    }

    #[test]
    fn confidence_roundtrip() {
        for c in [Confidence::High, Confidence::Medium, Confidence::Low] {
            let text = c.to_string();
            assert_eq!(Confidence::from_str(&text).unwrap(), c);
        }
    }

    #[test]
    fn parse_errors_for_unknown_values() {
        assert!(ClientMode::from_str("unknown").is_err());
        assert!(Severity::from_str("unknown").is_err());
        assert!(Health::from_str("unknown").is_err());
        assert!(Confidence::from_str("unknown").is_err());
    }

    #[test]
    fn diagnosis_context_builders() {
        let ctx = DiagnosisContext::new("5m")
            .with_model_name("llama")
            .with_client_mode(ClientMode::Scrape);
        assert_eq!(ctx.since, "5m");
        assert_eq!(ctx.model_name, Some("llama".to_string()));
        assert_eq!(ctx.client_mode, ClientMode::Scrape);
    }

    #[test]
    fn diagnosis_state_unknown_signal_message() {
        let state = DiagnosisState::unknown_signal(Signal::NumRequestsRunning);
        assert!(matches!(state, DiagnosisState::Unknown(_)));
        assert!(format!("{state:?}").contains("num_requests_running"));
    }

    #[test]
    fn finding_struct_is_independent() {
        let finding = Finding {
            severity: Severity::Warning,
            confidence: Confidence::High,
            title: "t".into(),
            summary: "s".into(),
            signals: vec!["a".into()],
            evidence: vec!["b".into()],
            likely_causes: vec!["c".into()],
            recommendations: vec!["d".into()],
            related_metrics: vec!["e".into()],
        };
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.signals, vec!["a".to_string()]);
    }
}
