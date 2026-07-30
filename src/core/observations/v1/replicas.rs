use std::collections::{BTreeMap, BTreeSet};

use crate::core::metrics::{ObservationDimension, REPLICA_LABELS, all_specs};
use crate::core::models::DiagnosisResult;

use super::types::ObservationBuildError;
use super::validation::MAX_REPLICAS;

pub(super) struct ReplicaAliases {
    label: &'static str,
    aliases: BTreeMap<String, String>,
}

impl ReplicaAliases {
    pub(super) fn from_result(
        result: &DiagnosisResult,
    ) -> Result<Option<Self>, ObservationBuildError> {
        let Some((label, values)) = REPLICA_LABELS.into_iter().find_map(|label| {
            let values = replica_values(result, label);
            (values.len() > 1).then_some((label, values))
        }) else {
            return Ok(None);
        };

        if values.len() > MAX_REPLICAS {
            return Err(ObservationBuildError::TooManyReplicas);
        }
        let aliases = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (value, format!("replica-{}", index + 1)))
            .collect();
        Ok(Some(Self { label, aliases }))
    }

    pub(super) fn label(&self) -> &'static str {
        self.label
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.aliases.iter()
    }
}

fn replica_values(result: &DiagnosisResult, label: &str) -> BTreeSet<String> {
    all_specs()
        .iter()
        .filter(|spec| {
            spec.observation_spec()
                .dimensions
                .contains(&ObservationDimension::Replica)
        })
        .filter_map(|spec| spec.series(&result.metric_series))
        .flat_map(|series| &series.samples)
        .filter_map(|sample| sample.labels.get(label).cloned())
        .collect()
}
