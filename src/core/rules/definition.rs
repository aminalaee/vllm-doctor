//! Static metadata for a diagnostic rule.
use crate::core::models::Severity;
use crate::core::rules::templates::FindingTemplate;

/// Immutable metadata describing a rule.
#[derive(Clone, Copy)]
pub struct RuleDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    pub likely_causes: &'static [&'static str],
    pub recommendations: &'static [&'static str],
    pub related_metrics: &'static [&'static str],
    pub template: &'static dyn FindingTemplate,
}

impl std::fmt::Debug for RuleDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleDefinition")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

impl PartialEq for RuleDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RuleDefinition {}
