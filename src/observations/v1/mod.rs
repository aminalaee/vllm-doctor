//! Version 1 of the outbound observation contract.

mod builder;
mod replicas;
mod types;
mod validation;

pub use builder::{ObservationBuildContext, build_batch};
pub use types::{
    AgentIdentityV1, AvailabilityStatusV1, AvailabilityV1, BottleneckV1, ConfidenceV1, HealthV1,
    InferenceEngineV1, LocalDiagnosisV1, MeasurementDimensionsV1, MeasurementKindV1,
    MeasurementRollupV1, MeasurementUnitV1, MeasurementV1, ObservationBatchV1,
    ObservationBuildError, TargetIdentityV1,
};
pub use validation::{
    MAX_IDENTIFIER_BYTES, MAX_OBSERVATIONS, MAX_REPLICAS, MAX_UNCOMPRESSED_JSON_BYTES,
};
