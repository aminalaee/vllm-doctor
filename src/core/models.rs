//! Domain models shared across the diagnostic engine.
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::core::metrics::MetricSeriesSnapshot;
use crate::core::signals::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsSource {
    #[default]
    Prometheus,
    DirectScrape,
}

impl fmt::Display for MetricsSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prometheus => write!(f, "prometheus"),
            Self::DirectScrape => write!(f, "direct_scrape"),
        }
    }
}

impl FromStr for MetricsSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prometheus" => Ok(Self::Prometheus),
            "direct_scrape" => Ok(Self::DirectScrape),
            _ => Err(format!("unknown metrics source: {s}")),
        }
    }
}

/// The inference engine serving the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceEngine {
    #[default]
    Vllm,
}

impl fmt::Display for InferenceEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vllm => write!(f, "vllm"),
        }
    }
}

impl FromStr for InferenceEngine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vllm" => Ok(Self::Vllm),
            _ => Err(format!("unknown inference engine: {s}")),
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
    #[serde(rename = "healthy")]
    Ok,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "critical")]
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TargetMetadata {
    pub id: Option<String>,
    pub engine: InferenceEngine,
    pub engine_version: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisContext {
    pub since: String,
    pub model_name: Option<String>,
    pub metrics_source: MetricsSource,
    pub target: TargetMetadata,
}

impl DiagnosisContext {
    pub fn new(since: impl Into<String>) -> Self {
        Self {
            since: since.into(),
            model_name: None,
            metrics_source: MetricsSource::default(),
            target: TargetMetadata::default(),
        }
    }

    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    pub fn with_metrics_source(mut self, source: MetricsSource) -> Self {
        self.metrics_source = source;
        self
    }

    pub fn with_target(mut self, target: TargetMetadata) -> Self {
        self.target = target;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct Finding {
    pub severity: Severity,
    pub confidence: Confidence,
    pub title: String,
    pub signals: Vec<String>,
    pub evidence: Vec<EvidenceItem>,
    pub likely_causes: Vec<String>,
    pub recommendations: Vec<String>,
    pub related_metrics: Vec<String>,
}

/// A structured piece of evidence for a finding. Rules produce data; the report
/// layer renders it into text or JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceItem {
    /// A metric compared against a fixed threshold.
    Threshold {
        metric: String,
        value: f64,
        threshold: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        #[serde(default)]
        operator: ComparisonOperator,
    },
    /// A raw metric value without a threshold.
    Value {
        metric: String,
        value: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// Distribution across replicas: how many of total are affected.
    ReplicaDistribution {
        affected: usize,
        total: usize,
        metric: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Free-form fallback for evidence that does not fit the typed variants.
    Text { message: String },
}

impl EvidenceItem {
    pub fn threshold(
        metric: impl Into<String>,
        value: f64,
        threshold: f64,
        unit: Option<impl Into<String>>,
        operator: ComparisonOperator,
    ) -> Self {
        Self::Threshold {
            metric: metric.into(),
            value,
            threshold,
            unit: unit.map(|u| u.into()),
            operator,
        }
    }

    pub fn value(metric: impl Into<String>, value: f64, unit: Option<impl Into<String>>) -> Self {
        Self::Value {
            metric: metric.into(),
            value,
            unit: unit.map(|u| u.into()),
        }
    }

    pub fn text(message: impl Into<String>) -> Self {
        Self::Text {
            message: message.into(),
        }
    }

    /// Dedup key based on the metric identifier.
    pub fn metric_key(&self) -> String {
        match self {
            EvidenceItem::Threshold { metric, .. } => metric.clone(),
            EvidenceItem::Value { metric, .. } => metric.clone(),
            EvidenceItem::ReplicaDistribution { metric, model, .. } => model
                .as_ref()
                .map(|model| format!("{model}:{metric}"))
                .unwrap_or_else(|| metric.clone()),
            EvidenceItem::Text { message } => message.clone(),
        }
    }

    /// Human-readable summary suitable for a compact one-line report.
    pub fn summary(&self) -> String {
        match self {
            EvidenceItem::Threshold {
                metric,
                value,
                threshold,
                unit,
                operator,
            } => format!(
                "{metric}: {} {} threshold {}",
                format_value_with_unit(*value, unit.as_deref()),
                operator.label(),
                format_value_with_unit(*threshold, unit.as_deref()),
            ),
            EvidenceItem::Value {
                metric,
                value,
                unit,
            } => format!(
                "{metric}: {}",
                format_value_with_unit(*value, unit.as_deref())
            ),
            EvidenceItem::ReplicaDistribution {
                affected,
                total,
                metric,
                model,
            } => {
                let model = model
                    .as_ref()
                    .map(|model| format!(" for {model}"))
                    .unwrap_or_default();
                format!("{affected}/{total} replicas show elevated {metric}{model}")
            }
            EvidenceItem::Text { message } => message.clone(),
        }
    }
}

/// How a measured value relates to its threshold.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    #[default]
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

impl ComparisonOperator {
    pub fn label(&self) -> &'static str {
        match self {
            Self::GreaterThan => ">",
            Self::GreaterThanOrEqual => "≥",
            Self::LessThan => "<",
            Self::LessThanOrEqual => "≤",
        }
    }
}

fn format_value(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1_000_000.0 {
        format!("{:.0}", value)
    } else {
        format!("{value:.2}")
    }
}

fn format_value_with_unit(value: f64, unit: Option<&str>) -> String {
    let value = format_value(value);
    match unit {
        None | Some("") => value,
        Some("%") => format!("{value}%"),
        Some(u) if u.len() == 1 => format!("{value}{u}"),
        Some(unit) => format!("{value} {unit}"),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
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

/// The most likely bottleneck category a diagnosis run points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckKind {
    QueueSaturation,
    KvCacheSaturation,
    LongPrefill,
    DecodeBottleneck,
    ReplicaImbalance,
    ErrorIssue,
    Idle,
    NoClearBottleneck,
}

impl BottleneckKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::QueueSaturation => "Queue saturation",
            Self::KvCacheSaturation => "KV cache saturation",
            Self::LongPrefill => "Long prefill / long input prompts",
            Self::DecodeBottleneck => "Decode / TPOT bottleneck",
            Self::ReplicaImbalance => "Replica imbalance",
            Self::ErrorIssue => "Error or failure issue",
            Self::Idle => "Idle / insufficient traffic",
            Self::NoClearBottleneck => "No clear bottleneck",
        }
    }
}

impl fmt::Display for BottleneckKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Root-cause interpretation of a diagnosis run: which bottleneck most likely
/// dominates, how sure we are, the evidence behind it, and what to do next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    pub likely_bottleneck: BottleneckKind,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceItem>,
    pub interpretation: String,
    pub recommended_next_actions: Vec<String>,
}

impl Default for Assessment {
    fn default() -> Self {
        Self {
            likely_bottleneck: BottleneckKind::NoClearBottleneck,
            confidence: Confidence::Low,
            evidence: Vec::new(),
            interpretation: String::new(),
            recommended_next_actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct DiagnosisResult {
    pub context: DiagnosisContext,
    pub metric_series: MetricSeriesSnapshot,
    pub checks: Vec<RuleResult>,
    #[serde(default)]
    pub assessment: Assessment,
}

impl DiagnosisResult {
    /// Assemble a result. The assessment defaults to "no clear bottleneck";
    /// the diagnosis pipeline fills it in via [`crate::core::assessment::assess`].
    pub fn new(
        context: DiagnosisContext,
        metric_series: MetricSeriesSnapshot,
        checks: Vec<RuleResult>,
    ) -> Self {
        Self {
            context,
            metric_series,
            checks,
            assessment: Assessment::default(),
        }
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
    fn metrics_source_roundtrip() {
        for source in [MetricsSource::Prometheus, MetricsSource::DirectScrape] {
            let text = source.to_string();
            assert_eq!(MetricsSource::from_str(&text).unwrap(), source);
        }
    }

    #[test]
    fn inference_engine_roundtrip() {
        let engine = InferenceEngine::Vllm;
        let text = engine.to_string();
        assert_eq!(InferenceEngine::from_str(&text).unwrap(), engine);
    }

    #[test]
    fn health_display_roundtrips_through_from_str() {
        for health in [Health::Ok, Health::Info, Health::Warning, Health::Critical] {
            let text = health.to_string();
            assert_eq!(Health::from_str(&text).unwrap(), health);
        }
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
        let result = DiagnosisResult::new(
            DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            vec![],
        );
        assert_eq!(result.health(), Health::Ok);
    }

    #[test]
    fn diagnosis_health_rolls_up_worst_finding() {
        let result = DiagnosisResult::new(
            DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            vec![
                RuleResult {
                    id: "info-rule".into(),
                    name: "Info Rule".into(),
                    title: "Info".into(),
                    severity: Severity::Info,
                    finding: Some(Finding {
                        severity: Severity::Info,
                        confidence: Confidence::Medium,
                        title: "Info".into(),
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
                        signals: vec![],
                        evidence: vec![],
                        likely_causes: vec![],
                        recommendations: vec![],
                        related_metrics: vec![],
                    }),
                },
            ],
        );
        assert_eq!(result.health(), Health::Critical);
    }

    #[test]
    fn diagnosis_health_ignores_none_findings() {
        let result = DiagnosisResult::new(
            DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            vec![RuleResult {
                id: "quiet".into(),
                name: "Quiet".into(),
                title: "Quiet".into(),
                severity: Severity::Info,
                finding: None,
            }],
        );
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
        assert!(MetricsSource::from_str("unknown").is_err());
        assert!(InferenceEngine::from_str("unknown").is_err());
        assert!(Severity::from_str("unknown").is_err());
        assert!(Health::from_str("unknown").is_err());
        assert!(Confidence::from_str("unknown").is_err());
    }

    #[test]
    fn diagnosis_context_builders() {
        let ctx = DiagnosisContext::new("5m")
            .with_model_name("llama")
            .with_metrics_source(MetricsSource::DirectScrape);
        assert_eq!(ctx.since, "5m");
        assert_eq!(ctx.model_name, Some("llama".to_string()));
        assert_eq!(ctx.metrics_source, MetricsSource::DirectScrape);
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
            signals: vec!["a".into()],
            evidence: vec![EvidenceItem::Text {
                message: "b".into(),
            }],
            likely_causes: vec!["c".into()],
            recommendations: vec!["d".into()],
            related_metrics: vec!["e".into()],
        };
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.signals, vec!["a".to_string()]);
    }
}
