use std::collections::{HashMap, HashSet};

use crate::core::metrics::{
    ObservationDimension, ObservationKind, ObservationRollup, ObservationUnit, all_specs,
};

use super::types::{
    MeasurementKindV1, MeasurementRollupV1, MeasurementUnitV1, ObservationBuildError,
    ObservationV1, ObservationValidationError, SCHEMA_VERSION,
};

pub const MAX_REPLICAS: usize = 64;
pub const MAX_OBSERVATIONS: usize = 512;
pub const MAX_IDENTIFIER_BYTES: usize = 128;
pub const MAX_UNCOMPRESSED_JSON_BYTES: usize = 256 * 1024;

pub(super) fn validate_inputs(
    target_id: &str,
    agent_id: &str,
    window_seconds: u64,
) -> Result<(), ObservationBuildError> {
    if window_seconds == 0 {
        return Err(ObservationBuildError::InvalidWindow(
            "window duration must be greater than zero".to_string(),
        ));
    }
    if target_id.trim().is_empty() {
        return Err(ObservationBuildError::InvalidTargetId);
    }
    if agent_id.trim().is_empty() {
        return Err(ObservationBuildError::InvalidAgentId);
    }
    Ok(())
}

pub fn validate_observation(observation: &ObservationV1) -> Result<(), ObservationValidationError> {
    if observation.schema_version() != SCHEMA_VERSION {
        return Err(ObservationValidationError::UnsupportedSchemaVersion);
    }

    if observation.window_seconds == 0 {
        return Err(ObservationValidationError::InvalidWindow);
    }
    if observation.target.id.trim().is_empty() {
        return Err(ObservationValidationError::InvalidTargetId);
    }
    if observation.agent.id.trim().is_empty() {
        return Err(ObservationValidationError::InvalidAgentId);
    }

    if observation.observations.len() > MAX_OBSERVATIONS {
        return Err(ObservationValidationError::TooManyObservations(
            MAX_OBSERVATIONS,
        ));
    }

    let replica_count = observation
        .observations
        .iter()
        .filter_map(|item| item.dimensions.as_ref())
        .map(|dimensions| dimensions.replica.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    if replica_count > MAX_REPLICAS {
        return Err(ObservationValidationError::TooManyReplicas(MAX_REPLICAS));
    }

    let identifiers: Vec<&str> = [
        observation.agent.id.as_str(),
        observation.agent.version.as_str(),
        observation.agent.local_rule_pack.as_str(),
        observation.target.id.as_str(),
    ]
    .into_iter()
    .chain(observation.target.engine_version.as_deref())
    .chain(observation.target.environment.as_deref())
    .chain(observation.target.model.as_deref())
    .chain(
        observation
            .local_diagnosis
            .firing_rule_ids
            .iter()
            .map(String::as_str),
    )
    .chain(observation.observations.iter().map(|item| item.id.as_str()))
    .chain(observation.observations.iter().filter_map(|item| {
        item.dimensions
            .as_ref()
            .map(|dimensions| dimensions.replica.as_str())
    }))
    .chain(observation.availability.iter().map(|item| item.id.as_str()))
    .collect();

    if identifiers.iter().any(|value| value.trim().is_empty()) {
        return Err(ObservationValidationError::InvalidIdentifier);
    }
    if identifiers
        .iter()
        .any(|value| value.len() > MAX_IDENTIFIER_BYTES)
    {
        return Err(ObservationValidationError::IdentifierTooLong(
            MAX_IDENTIFIER_BYTES,
        ));
    }
    if observation.observations.iter().any(|item| {
        !item.value.is_finite() || item.quantile.is_some_and(|value| !value.is_finite())
    }) {
        return Err(ObservationValidationError::NonFiniteValue);
    }
    if serde_json::to_vec(observation)
        .map_err(|_| ObservationValidationError::PayloadTooLarge(MAX_UNCOMPRESSED_JSON_BYTES))?
        .len()
        > MAX_UNCOMPRESSED_JSON_BYTES
    {
        return Err(ObservationValidationError::PayloadTooLarge(
            MAX_UNCOMPRESSED_JSON_BYTES,
        ));
    }

    validate_measurements(observation)?;
    Ok(())
}

fn validate_measurements(observation: &ObservationV1) -> Result<(), ObservationValidationError> {
    let specs: HashMap<_, _> = all_specs()
        .iter()
        .map(|spec| (spec.observation_spec().id, spec.observation_spec()))
        .collect();
    let mut aggregates = HashSet::new();
    let mut replicas = HashSet::new();
    let mut measured = HashSet::new();

    for measurement in &observation.observations {
        let Some(spec) = specs.get(measurement.id.as_str()).copied() else {
            return Err(ObservationValidationError::UnknownMeasurement(
                measurement.id.clone(),
            ));
        };
        validate_metadata(measurement, spec)?;

        measured.insert(measurement.id.as_str());
        if let Some(dimensions) = &measurement.dimensions {
            if !spec.dimensions.contains(&ObservationDimension::Replica) {
                return Err(ObservationValidationError::InvalidDimensions(
                    measurement.id.clone(),
                ));
            }
            if !replicas.insert((measurement.id.as_str(), dimensions.replica.as_str())) {
                return Err(ObservationValidationError::DuplicateReplica {
                    id: measurement.id.clone(),
                    replica: dimensions.replica.clone(),
                });
            }
        } else if !aggregates.insert(measurement.id.as_str()) {
            return Err(ObservationValidationError::DuplicateAggregate(
                measurement.id.clone(),
            ));
        }
    }

    let mut unavailable = HashSet::new();
    for availability in &observation.availability {
        if !specs.contains_key(availability.id.as_str()) {
            return Err(ObservationValidationError::UnknownMeasurement(
                availability.id.clone(),
            ));
        }
        if !unavailable.insert(availability.id.as_str()) {
            return Err(ObservationValidationError::DuplicateAvailability(
                availability.id.clone(),
            ));
        }
        if measured.contains(availability.id.as_str()) {
            return Err(ObservationValidationError::ConflictingAvailability(
                availability.id.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_metadata(
    measurement: &super::types::MeasurementV1,
    spec: &crate::core::metrics::ObservationSpec,
) -> Result<(), ObservationValidationError> {
    let id = || measurement.id.clone();
    let mismatch = |field| ObservationValidationError::MetadataMismatch { id: id(), field };

    let unit = match spec.unit {
        ObservationUnit::Count => MeasurementUnitV1::Count,
        ObservationUnit::Ratio => MeasurementUnitV1::Ratio,
        ObservationUnit::Seconds => MeasurementUnitV1::Seconds,
        ObservationUnit::TokensPerSecond => MeasurementUnitV1::TokensPerSecond,
    };
    if measurement.unit != unit {
        return Err(mismatch("unit"));
    }

    let kind = match spec.kind {
        ObservationKind::Gauge => MeasurementKindV1::Gauge,
        ObservationKind::WindowDelta => MeasurementKindV1::WindowDelta,
        ObservationKind::Quantile => MeasurementKindV1::Quantile,
        ObservationKind::WindowRatio => MeasurementKindV1::WindowRatio,
    };
    if measurement.kind != kind {
        return Err(mismatch("kind"));
    }

    let rollup = match spec.rollup {
        ObservationRollup::Sum => MeasurementRollupV1::Sum,
        ObservationRollup::Max => MeasurementRollupV1::Max,
        ObservationRollup::Ratio => MeasurementRollupV1::Ratio,
    };
    if measurement.rollup != rollup {
        return Err(mismatch("rollup"));
    }
    if measurement.quantile != spec.quantile {
        return Err(mismatch("quantile"));
    }
    Ok(())
}

impl From<ObservationValidationError> for ObservationBuildError {
    fn from(error: ObservationValidationError) -> Self {
        match error {
            ObservationValidationError::UnsupportedSchemaVersion => Self::UnsupportedSchemaVersion,
            ObservationValidationError::InvalidTargetId => Self::InvalidTargetId,
            ObservationValidationError::InvalidAgentId => Self::InvalidAgentId,
            ObservationValidationError::InvalidWindow => {
                Self::InvalidWindow("window duration must be greater than zero".to_string())
            }
            ObservationValidationError::InvalidIdentifier => Self::InvalidIdentifier,
            ObservationValidationError::IdentifierTooLong(_) => Self::IdentifierTooLong,
            ObservationValidationError::TooManyReplicas(_) => Self::TooManyReplicas,
            ObservationValidationError::TooManyObservations(_) => Self::TooManyObservations,
            ObservationValidationError::NonFiniteValue => Self::NonFiniteValue,
            ObservationValidationError::PayloadTooLarge(_) => Self::PayloadTooLarge,
            other => Self::InvalidObservation(other.to_string()),
        }
    }
}
