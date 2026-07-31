//! Conversion from a diagnosis result to the versioned observation contract.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::metrics::{
    ObservationDimension, ObservationKind, ObservationRollup, ObservationSpec, ObservationUnit,
    all_specs,
};
use crate::core::models::{
    BottleneckKind, Confidence, DiagnosisResult, Health, InferenceEngine, MetricsSource,
};

use super::replicas::ReplicaAliases;
use super::types::{
    AgentIdentityV1, AvailabilityStatusV1, AvailabilityV1, BottleneckV1, ConfidenceV1, HealthV1,
    InferenceEngineV1, LocalDiagnosisV1, MeasurementDimensionsV1, MeasurementKindV1,
    MeasurementRollupV1, MeasurementUnitV1, MeasurementV1, MetricsSourceV1, ObservationBuildError,
    ObservationV1, TargetIdentityV1,
};
use super::validation::{validate_inputs, validate_observation};

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationBuildContext {
    pub event_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub agent_id: String,
    pub agent_version: String,
    pub local_rule_pack: String,
}

pub fn build_observation(
    result: &DiagnosisResult,
    context: &ObservationBuildContext,
    window_seconds: u64,
) -> Result<ObservationV1, ObservationBuildError> {
    let target_id = result
        .context
        .target
        .id
        .as_deref()
        .ok_or(ObservationBuildError::MissingTargetId)?;
    validate_inputs(target_id, &context.agent_id, window_seconds)?;

    let replicas = ReplicaAliases::from_result(result)?;
    let (observations, availability) = collect_observations(result, replicas.as_ref());
    let observation = ObservationV1::new(
        context.event_id,
        context.observed_at,
        window_seconds,
        match result.context.metrics_source {
            MetricsSource::Prometheus => MetricsSourceV1::Prometheus,
            MetricsSource::DirectScrape => MetricsSourceV1::DirectScrape,
        },
        build_agent(context),
        build_target(result, target_id),
        observations,
        availability,
        build_local_diagnosis(result),
    );
    validate_observation(&observation)?;
    Ok(observation)
}

fn collect_observations(
    result: &DiagnosisResult,
    replicas: Option<&ReplicaAliases>,
) -> (Vec<MeasurementV1>, Vec<AvailabilityV1>) {
    let mut measurements = Vec::new();
    let mut availability = Vec::new();

    for spec in all_specs() {
        let observation = spec.observation_spec();
        if result.context.metrics_source == MetricsSource::DirectScrape
            && observation.requires_promql()
        {
            availability.push(not_collected(observation));
            continue;
        }

        match spec.extract(&result.metric_series) {
            Some(value) if value.is_finite() => {
                measurements.push(measurement(observation, None, value));
            }
            _ => availability.push(not_collected(observation)),
        }

        if observation
            .dimensions
            .contains(&ObservationDimension::Replica)
        {
            if let (Some(replicas), Some(series)) = (replicas, spec.series(&result.metric_series)) {
                let grouped = series.by(replicas.label());
                for (raw_value, alias) in replicas.iter() {
                    if let Some(Some(value)) = grouped.get(raw_value).copied() {
                        if value.is_finite() {
                            measurements.push(measurement(
                                observation,
                                Some(MeasurementDimensionsV1 {
                                    replica: alias.clone(),
                                }),
                                value,
                            ));
                        }
                    }
                }
            }
        }
    }
    (measurements, availability)
}

fn measurement(
    observation: &ObservationSpec,
    dimensions: Option<MeasurementDimensionsV1>,
    value: f64,
) -> MeasurementV1 {
    MeasurementV1 {
        id: observation.id.to_string(),
        unit: match observation.unit {
            ObservationUnit::Count => MeasurementUnitV1::Count,
            ObservationUnit::Ratio => MeasurementUnitV1::Ratio,
            ObservationUnit::Seconds => MeasurementUnitV1::Seconds,
            ObservationUnit::TokensPerSecond => MeasurementUnitV1::TokensPerSecond,
        },
        kind: match observation.kind {
            ObservationKind::Gauge => MeasurementKindV1::Gauge,
            ObservationKind::WindowDelta => MeasurementKindV1::WindowDelta,
            ObservationKind::Quantile => MeasurementKindV1::Quantile,
            ObservationKind::WindowRatio => MeasurementKindV1::WindowRatio,
        },
        rollup: match observation.rollup {
            ObservationRollup::Sum => MeasurementRollupV1::Sum,
            ObservationRollup::Max => MeasurementRollupV1::Max,
            ObservationRollup::Ratio => MeasurementRollupV1::Ratio,
        },
        quantile: observation.quantile,
        dimensions,
        value,
    }
}

fn not_collected(observation: &ObservationSpec) -> AvailabilityV1 {
    AvailabilityV1 {
        id: observation.id.to_string(),
        status: AvailabilityStatusV1::NotCollected,
    }
}

fn build_agent(context: &ObservationBuildContext) -> AgentIdentityV1 {
    AgentIdentityV1 {
        id: context.agent_id.clone(),
        version: context.agent_version.clone(),
        local_rule_pack: context.local_rule_pack.clone(),
    }
}

fn build_target(result: &DiagnosisResult, target_id: &str) -> TargetIdentityV1 {
    TargetIdentityV1 {
        id: target_id.to_string(),
        engine: match result.context.target.engine {
            InferenceEngine::Vllm => InferenceEngineV1::Vllm,
        },
        engine_version: result.context.target.engine_version.clone(),
        environment: result.context.target.environment.clone(),
        model: result.context.model_name.clone(),
    }
}

fn build_local_diagnosis(result: &DiagnosisResult) -> LocalDiagnosisV1 {
    let mut firing_rule_ids: Vec<String> = result
        .checks
        .iter()
        .filter(|check| check.finding.is_some())
        .map(|check| check.id.clone())
        .collect();
    firing_rule_ids.sort();

    LocalDiagnosisV1 {
        health: match result.health() {
            Health::Ok => HealthV1::Healthy,
            Health::Info => HealthV1::Info,
            Health::Warning => HealthV1::Warning,
            Health::Critical => HealthV1::Critical,
        },
        likely_bottleneck: match result.assessment.likely_bottleneck {
            BottleneckKind::QueueSaturation => BottleneckV1::QueueSaturation,
            BottleneckKind::KvCacheSaturation => BottleneckV1::KvCacheSaturation,
            BottleneckKind::LongPrefill => BottleneckV1::LongPrefill,
            BottleneckKind::DecodeBottleneck => BottleneckV1::DecodeBottleneck,
            BottleneckKind::ReplicaImbalance => BottleneckV1::ReplicaImbalance,
            BottleneckKind::ErrorIssue => BottleneckV1::ErrorIssue,
            BottleneckKind::Idle => BottleneckV1::Idle,
            BottleneckKind::NoClearBottleneck => BottleneckV1::NoClearBottleneck,
        },
        confidence: match result.assessment.confidence {
            Confidence::High => ConfidenceV1::High,
            Confidence::Medium => ConfidenceV1::Medium,
            Confidence::Low => ConfidenceV1::Low,
        },
        firing_rule_ids,
    }
}
