use super::types::{ObservationBuildError, ObservationV1, SCHEMA_VERSION};

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

pub fn validate_observation(observation: &ObservationV1) -> Result<(), ObservationBuildError> {
    if observation.schema_version() != SCHEMA_VERSION {
        return Err(ObservationBuildError::UnsupportedSchemaVersion);
    }

    validate_inputs(
        &observation.target.id,
        &observation.agent.id,
        observation.window_seconds,
    )?;

    if observation.observations.len() > MAX_OBSERVATIONS {
        return Err(ObservationBuildError::TooManyObservations);
    }

    let replica_count = observation
        .observations
        .iter()
        .filter_map(|item| item.dimensions.as_ref())
        .map(|dimensions| dimensions.replica.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    if replica_count > MAX_REPLICAS {
        return Err(ObservationBuildError::TooManyReplicas);
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
        return Err(ObservationBuildError::InvalidIdentifier);
    }
    if identifiers
        .iter()
        .any(|value| value.len() > MAX_IDENTIFIER_BYTES)
    {
        return Err(ObservationBuildError::IdentifierTooLong);
    }
    if observation.observations.iter().any(|item| {
        !item.value.is_finite() || item.quantile.is_some_and(|value| !value.is_finite())
    }) {
        return Err(ObservationBuildError::NonFiniteValue);
    }
    if serde_json::to_vec(observation)
        .map_err(|_| ObservationBuildError::PayloadTooLarge)?
        .len()
        > MAX_UNCOMPRESSED_JSON_BYTES
    {
        return Err(ObservationBuildError::PayloadTooLarge);
    }
    Ok(())
}
