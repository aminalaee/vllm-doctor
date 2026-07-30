//! Typed `ObservationV1` wire structures.
//!
//! These types are intentionally separate from the diagnostic domain model.
//! Their wire representation cannot change when internal display or debug
//! representations change.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub(super) const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ObservationV1 {
    schema_version: u32,
    pub event_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub window_seconds: u64,
    pub agent: AgentIdentityV1,
    pub target: TargetIdentityV1,
    pub observations: Vec<MeasurementV1>,
    pub availability: Vec<AvailabilityV1>,
    pub local_diagnosis: LocalDiagnosisV1,
}

impl ObservationV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        event_id: Uuid,
        observed_at: DateTime<Utc>,
        window_seconds: u64,
        agent: AgentIdentityV1,
        target: TargetIdentityV1,
        observations: Vec<MeasurementV1>,
        availability: Vec<AvailabilityV1>,
        local_diagnosis: LocalDiagnosisV1,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            event_id,
            observed_at,
            window_seconds,
            agent,
            target,
            observations,
            availability,
            local_diagnosis,
        }
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AgentIdentityV1 {
    pub id: String,
    pub version: String,
    pub local_rule_pack: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceEngineV1 {
    Vllm,
}

impl InferenceEngineV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vllm => "vllm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TargetIdentityV1 {
    pub id: String,
    pub engine: InferenceEngineV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnitV1 {
    Count,
    Ratio,
    Seconds,
    TokensPerSecond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementKindV1 {
    Gauge,
    WindowDelta,
    Quantile,
    WindowRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementRollupV1 {
    Sum,
    Max,
    Ratio,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MeasurementV1 {
    pub id: String,
    pub unit: MeasurementUnitV1,
    pub kind: MeasurementKindV1,
    pub rollup: MeasurementRollupV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantile: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<MeasurementDimensionsV1>,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MeasurementDimensionsV1 {
    pub replica: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatusV1 {
    NotCollected,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AvailabilityV1 {
    pub id: String,
    pub status: AvailabilityStatusV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthV1 {
    Healthy,
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceV1 {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckV1 {
    QueueSaturation,
    KvCacheSaturation,
    LongPrefill,
    DecodeBottleneck,
    ReplicaImbalance,
    ErrorIssue,
    Idle,
    NoClearBottleneck,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LocalDiagnosisV1 {
    pub health: HealthV1,
    pub likely_bottleneck: BottleneckV1,
    pub confidence: ConfidenceV1,
    pub firing_rule_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObservationBuildError {
    #[error("unsupported observation schema version")]
    UnsupportedSchemaVersion,
    #[error("missing target id: target.id is required to build an observation")]
    MissingTargetId,
    #[error("invalid target id: id is empty or whitespace-only")]
    InvalidTargetId,
    #[error("invalid agent id: id is empty or whitespace-only")]
    InvalidAgentId,
    #[error("observation contains an empty identifier")]
    InvalidIdentifier,
    #[error("invalid observation window: {0}")]
    InvalidWindow(String),
    #[error(
        "too many replicas: count exceeds maximum of {}",
        super::validation::MAX_REPLICAS
    )]
    TooManyReplicas,
    #[error(
        "too many observations: count exceeds maximum of {}",
        super::validation::MAX_OBSERVATIONS
    )]
    TooManyObservations,
    #[error("observation contains a non-finite numeric value")]
    NonFiniteValue,
    #[error(
        "identifier exceeds maximum of {} bytes",
        super::validation::MAX_IDENTIFIER_BYTES
    )]
    IdentifierTooLong,
    #[error(
        "payload exceeds maximum of {} bytes",
        super::validation::MAX_UNCOMPRESSED_JSON_BYTES
    )]
    PayloadTooLarge,
}
