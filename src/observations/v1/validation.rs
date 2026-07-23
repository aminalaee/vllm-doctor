use super::types::{ObservationBatchV1, ObservationBuildError};

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

pub(super) fn validate_batch(batch: &ObservationBatchV1) -> Result<(), ObservationBuildError> {
    if batch.observations.len() > MAX_OBSERVATIONS {
        return Err(ObservationBuildError::TooManyObservations);
    }

    let identifiers = [
        batch.agent.id.as_str(),
        batch.agent.version.as_str(),
        batch.agent.local_rule_pack.as_str(),
        batch.target.id.as_str(),
    ]
    .into_iter()
    .chain(batch.target.engine_version.as_deref())
    .chain(batch.target.environment.as_deref())
    .chain(batch.target.model.as_deref())
    .chain(
        batch
            .local_diagnosis
            .firing_rule_ids
            .iter()
            .map(String::as_str),
    )
    .chain(batch.observations.iter().map(|item| item.id.as_str()))
    .chain(batch.observations.iter().filter_map(|item| {
        item.dimensions
            .as_ref()
            .map(|dimensions| dimensions.replica.as_str())
    }))
    .chain(batch.availability.iter().map(|item| item.id.as_str()));

    if identifiers
        .into_iter()
        .any(|value| value.len() > MAX_IDENTIFIER_BYTES)
    {
        return Err(ObservationBuildError::IdentifierTooLong);
    }
    if serde_json::to_vec(batch)
        .map_err(|_| ObservationBuildError::PayloadTooLarge)?
        .len()
        > MAX_UNCOMPRESSED_JSON_BYTES
    {
        return Err(ObservationBuildError::PayloadTooLarge);
    }
    Ok(())
}
