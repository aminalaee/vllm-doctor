//! Static metadata for a diagnostic rule.
use crate::models::Severity;

/// Immutable metadata describing a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    pub likely_causes: &'static [&'static str],
    pub recommendations: &'static [&'static str],
    pub related_metrics: &'static [&'static str],
}
