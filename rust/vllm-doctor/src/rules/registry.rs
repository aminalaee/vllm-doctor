//! Rule registry: collect, filter, and execute rules.
use crate::config::Config;
use crate::metrics::MetricSeriesSnapshot;
use crate::models::Severity;
use crate::signals::SignalGraph;

use super::definition::RuleDefinition;
use super::{Rule, finding_for};
use crate::models::RuleResult;

/// Factory function signature for registering a rule type.
pub type RuleFactory = fn(&Config) -> (&'static RuleDefinition, Box<dyn Rule>);

struct RegistryEntry {
    definition: &'static RuleDefinition,
    rule: Box<dyn Rule>,
}

/// Central collection of configured diagnostic rules.
#[derive(Default)]
pub struct RuleRegistry {
    entries: Vec<RegistryEntry>,
}

impl RuleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a rule type using its factory.
    pub fn register(mut self, factory: RuleFactory, config: &Config) -> Self {
        let (definition, rule) = factory(config);
        self.entries.push(RegistryEntry { definition, rule });
        self
    }

    /// All registered rule definitions.
    pub fn definitions(&self) -> impl Iterator<Item = &&'static RuleDefinition> {
        self.entries.iter().map(|e| &e.definition)
    }

    /// Filter definitions by severity.
    pub fn definitions_by_severity(
        &self,
        severity: Severity,
    ) -> impl Iterator<Item = &&'static RuleDefinition> {
        self.definitions().filter(move |d| d.severity == severity)
    }

    /// Run every rule and collect metadata + findings.
    pub fn run_all(&self, metrics: &MetricSeriesSnapshot) -> Vec<RuleResult> {
        let signals = SignalGraph::new(metrics);
        let mut results: Vec<RuleResult> = self
            .entries
            .iter()
            .map(|entry| RuleResult {
                id: entry.definition.id,
                name: entry.definition.name,
                title: entry.definition.title,
                severity: entry.definition.severity,
                finding: finding_for(entry.definition, entry.rule.run(&signals), &signals),
            })
            .collect();
        results.sort_by(|a, b| {
            a.severity
                .cmp(&b.severity)
                .then_with(|| b.is_significant().cmp(&a.is_significant()))
        });
        results
    }
}
