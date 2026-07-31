use std::collections::HashMap;

use crate::core::metrics::{
    Aggregate, MODEL_LABEL, MetricSample, MetricSeries, MetricSeriesSnapshot, ObservationRollup,
    all_specs,
};
use crate::core::models::{DiagnosisContext, InferenceEngine, MetricsSource, TargetMetadata};

use super::types::{
    InferenceEngineV1, MeasurementV1, MetricsSourceV1, ObservationV1, ObservationValidationError,
};
use super::validation::validate_observation;

pub fn reconstruct_snapshot(
    observation: &ObservationV1,
) -> Result<MetricSeriesSnapshot, ObservationValidationError> {
    validate_observation(observation)?;

    let mut by_id: HashMap<&str, Vec<&MeasurementV1>> = HashMap::new();
    for measurement in &observation.observations {
        by_id
            .entry(measurement.id.as_str())
            .or_default()
            .push(measurement);
    }

    let mut snapshot = MetricSeriesSnapshot::default();
    for spec in all_specs() {
        let observation_spec = spec.observation_spec();
        let Some(measurements) = by_id.get(observation_spec.id) else {
            continue;
        };

        let aggregate = measurements
            .iter()
            .find(|measurement| measurement.dimensions.is_none());
        let mut samples = Vec::with_capacity(measurements.len());
        if let Some(measurement) = aggregate {
            let mut sample = MetricSample::new(measurement.value);
            if let Some(model) = &observation.target.model {
                sample = sample.with_label(MODEL_LABEL, model);
            }
            samples.push(sample);
        }
        samples.extend(measurements.iter().filter_map(|measurement| {
            measurement.dimensions.as_ref().map(|dimensions| {
                let mut sample = MetricSample::new(measurement.value)
                    .with_label("replica", dimensions.replica.clone());
                if let Some(model) = &observation.target.model {
                    sample = sample.with_label(MODEL_LABEL, model);
                }
                sample
            })
        }));

        let aggregate_by = if aggregate.is_some() {
            Aggregate::Authoritative
        } else {
            match observation_spec.rollup {
                ObservationRollup::Sum | ObservationRollup::Ratio => Aggregate::Sum,
                ObservationRollup::Max => Aggregate::Max,
            }
        };
        let inserted = snapshot.set_series(
            spec.output(),
            MetricSeries {
                samples,
                aggregate_by,
            },
        );
        debug_assert!(inserted, "registered metric output must exist in snapshot");
    }
    Ok(snapshot)
}

pub fn diagnosis_context(observation: &ObservationV1) -> DiagnosisContext {
    let mut context = DiagnosisContext::new(format!("{}s", observation.window_seconds))
        .with_metrics_source(match observation.metrics_source {
            MetricsSourceV1::Prometheus => MetricsSource::Prometheus,
            MetricsSourceV1::DirectScrape => MetricsSource::DirectScrape,
        })
        .with_target(TargetMetadata {
            id: Some(observation.target.id.clone()),
            engine: match observation.target.engine {
                InferenceEngineV1::Vllm => InferenceEngine::Vllm,
            },
            engine_version: observation.target.engine_version.clone(),
            environment: observation.target.environment.clone(),
        });
    context.model_name.clone_from(&observation.target.model);
    context
}
