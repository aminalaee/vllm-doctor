//! Labeled metric samples, series, and aggregation strategies.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub labels: HashMap<String, String>,
    pub value: f64,
    pub timestamp: Option<f64>,
}

impl MetricSample {
    pub fn new(value: f64) -> Self {
        Self {
            labels: HashMap::new(),
            value,
            timestamp: None,
        }
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_timestamp(mut self, timestamp: f64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregate {
    #[default]
    Sum,
    Max,
    Avg,
}

impl Aggregate {
    pub fn apply(&self, samples: &[MetricSample]) -> Option<f64> {
        if samples.is_empty() {
            return None;
        }
        match self {
            Aggregate::Sum => Some(samples.iter().map(|s| s.value).sum()),
            Aggregate::Max => samples
                .iter()
                .map(|s| s.value)
                .max_by(|a, b| a.total_cmp(b)),
            Aggregate::Avg => {
                let sum: f64 = samples.iter().map(|s| s.value).sum();
                Some(sum / samples.len() as f64)
            }
        }
    }
}

impl std::fmt::Display for Aggregate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sum => write!(f, "sum"),
            Self::Max => write!(f, "max"),
            Self::Avg => write!(f, "avg"),
        }
    }
}

impl std::str::FromStr for Aggregate {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sum" => Ok(Self::Sum),
            "max" => Ok(Self::Max),
            "avg" => Ok(Self::Avg),
            _ => Err(format!("unknown aggregate: {s}")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSeries {
    pub samples: Vec<MetricSample>,
    #[serde(skip)]
    pub aggregate_by: Aggregate,
}

impl MetricSeries {
    pub fn scalar(value: f64) -> Self {
        Self {
            samples: vec![MetricSample::new(value)],
            aggregate_by: Aggregate::Sum,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn value(&self) -> Option<f64> {
        self.aggregate_by.apply(&self.samples)
    }

    pub fn by(&self, dim: &str) -> HashMap<String, Option<f64>> {
        let mut groups: HashMap<String, Vec<MetricSample>> = HashMap::new();
        for sample in &self.samples {
            if let Some(key) = sample.labels.get(dim) {
                groups.entry(key.clone()).or_default().push(sample.clone());
            }
        }
        groups
            .into_iter()
            .map(|(key, samples)| {
                let value = self.aggregate_by.apply(&samples);
                (key, value)
            })
            .collect()
    }

    pub fn filter(&self, labels: &HashMap<String, String>) -> Self {
        Self {
            samples: self
                .samples
                .iter()
                .filter(|sample| labels.iter().all(|(k, v)| sample.labels.get(k) == Some(v)))
                .cloned()
                .collect(),
            aggregate_by: self.aggregate_by,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(value: f64, labels: &[(&str, &str)]) -> MetricSample {
        MetricSample {
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            value,
            timestamp: None,
        }
    }

    #[test]
    fn metric_series_from_scalar() {
        let series = MetricSeries::scalar(2.5);
        assert_eq!(series.samples, vec![MetricSample::new(2.5)]);
        assert_eq!(series.value(), Some(2.5));
    }

    #[test]
    fn empty_metric_series_has_no_value() {
        let series = MetricSeries::empty();
        assert!(series.value().is_none());
    }

    #[test]
    fn metric_series_aggregates_samples() {
        let series = MetricSeries {
            samples: vec![sample(2.0, &[("pod", "a")]), sample(4.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Sum,
        };
        assert_eq!(series.value(), Some(6.0));

        let max_series = MetricSeries {
            samples: series.samples.clone(),
            aggregate_by: Aggregate::Max,
        };
        assert_eq!(max_series.value(), Some(4.0));

        let avg_series = MetricSeries {
            samples: series.samples.clone(),
            aggregate_by: Aggregate::Avg,
        };
        assert_eq!(avg_series.value(), Some(3.0));
    }

    #[test]
    fn metric_series_groups_by_label() {
        let series = MetricSeries {
            samples: vec![
                sample(2.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(3.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(4.0, &[("pod", "b"), ("model_name", "llama")]),
                sample(5.0, &[("model_name", "llama")]),
            ],
            aggregate_by: Aggregate::Sum,
        };

        let grouped = series.by("pod");
        assert_eq!(grouped.get("a").copied().flatten(), Some(5.0));
        assert_eq!(grouped.get("b").copied().flatten(), Some(4.0));
    }

    #[test]
    fn metric_series_filters_by_labels() {
        let series = MetricSeries {
            samples: vec![
                sample(2.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(3.0, &[("pod", "b"), ("model_name", "llama")]),
                sample(4.0, &[("pod", "a"), ("model_name", "mistral")]),
            ],
            aggregate_by: Aggregate::Sum,
        };

        let mut labels = HashMap::new();
        labels.insert("pod".to_string(), "a".to_string());
        labels.insert("model_name".to_string(), "llama".to_string());
        let filtered = series.filter(&labels);

        assert_eq!(filtered.samples.len(), 1);
        assert_eq!(filtered.samples[0].value, 2.0);
    }
}
