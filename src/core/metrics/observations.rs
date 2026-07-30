//! Stable, engine-neutral observation metadata for the cloud-facing contract.
//!
//! These types describe the meaning of each collected metric independent of
//! Rust field names, vLLM exposition names, and terminal display formatting.
//! They are the beginning of a versioned external contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationUnit {
    Count,
    Ratio,
    Seconds,
    TokensPerSecond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationKind {
    Gauge,
    WindowDelta,
    Quantile,
    WindowRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationRollup {
    Sum,
    Max,
    Ratio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationDimension {
    Replica,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObservationSpec {
    pub id: &'static str,
    pub unit: ObservationUnit,
    pub kind: ObservationKind,
    pub rollup: ObservationRollup,
    pub quantile: Option<f64>,
    pub dimensions: &'static [ObservationDimension],
}

impl ObservationSpec {
    /// Returns `true` when direct scraping cannot provide this observation with
    /// its declared window semantics.
    ///
    /// Direct scraping exposes cumulative counters instead of window deltas and
    /// cannot run histogram quantile queries. Callers must therefore treat
    /// these observation kinds as unavailable rather than exporting the raw
    /// fallback values under incorrect semantics.
    pub fn requires_promql(&self) -> bool {
        matches!(
            self.kind,
            ObservationKind::WindowDelta | ObservationKind::Quantile | ObservationKind::WindowRatio
        )
    }
}

pub const NO_DIMENSIONS: &[ObservationDimension] = &[];
pub const REPLICA_DIMENSION: &[ObservationDimension] = &[ObservationDimension::Replica];

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ObservationKind) -> ObservationSpec {
        ObservationSpec {
            id: "test",
            unit: ObservationUnit::Count,
            kind,
            rollup: ObservationRollup::Sum,
            quantile: None,
            dimensions: NO_DIMENSIONS,
        }
    }

    #[test]
    fn promql_requirement_covers_all_window_derived_observations() {
        assert!(!spec(ObservationKind::Gauge).requires_promql());
        assert!(spec(ObservationKind::WindowDelta).requires_promql());
        assert!(spec(ObservationKind::Quantile).requires_promql());
        assert!(spec(ObservationKind::WindowRatio).requires_promql());
    }
}
