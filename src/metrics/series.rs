//! Labeled metric samples, series, and aggregation strategies.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub labels: HashMap<String, String>,
    #[serde(with = "finite_f64")]
    pub value: f64,
    pub timestamp: Option<f64>,
}

/// Serde for an `f64` that may be non-finite: finite values stay JSON numbers;
/// `NaN`/`Inf`/`-Inf` are written as strings and parsed back.
mod finite_f64 {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        if value.is_finite() {
            serializer.serialize_f64(*value)
        } else if value.is_nan() {
            serializer.serialize_str("nan")
        } else if value.is_sign_positive() {
            serializer.serialize_str("inf")
        } else {
            serializer.serialize_str("-inf")
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Num(f64),
            Str(String),
        }
        match Repr::deserialize(deserializer)? {
            Repr::Num(n) => Ok(n),
            Repr::Str(s) => match s.as_str() {
                "nan" => Ok(f64::NAN),
                "inf" => Ok(f64::INFINITY),
                "-inf" => Ok(f64::NEG_INFINITY),
                other => other.parse().map_err(D::Error::custom),
            },
        }
    }
}

impl MetricSample {
    pub fn new(value: f64) -> Self {
        Self {
            labels: HashMap::new(),
            value,
            timestamp: None,
        }
    }

    pub fn scalar(value: f64) -> Self {
        Self::new(value)
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

/// Samples and their aggregation strategy. The strategy is serialized so
/// stored gauge series do not reload with the default `Sum` aggregation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSeries {
    pub samples: Vec<MetricSample>,
    pub aggregate_by: Aggregate,
}

impl MetricSeries {
    pub fn scalar(value: f64) -> Self {
        Self {
            samples: vec![MetricSample::new(value)],
            aggregate_by: Aggregate::Sum,
        }
    }

    pub fn from_samples(samples: Vec<MetricSample>) -> Self {
        Self {
            samples,
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
    use std::collections::HashMap;
    use std::str::FromStr;

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

    #[test]
    fn aggregate_display_and_parse() {
        for agg in [Aggregate::Sum, Aggregate::Max, Aggregate::Avg] {
            assert_eq!(Aggregate::from_str(&agg.to_string()).unwrap(), agg);
        }
    }

    #[test]
    fn aggregate_parse_rejects_unknown() {
        assert!(Aggregate::from_str("median").is_err());
    }

    #[test]
    fn by_returns_none_for_missing_dimension() {
        let series = MetricSeries {
            samples: vec![sample(2.0, &[("pod", "a")]), sample(3.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Sum,
        };
        assert!(series.by("model_name").is_empty());
    }

    #[test]
    fn filter_preserves_aggregate_by() {
        let series = MetricSeries {
            samples: vec![sample(2.0, &[("pod", "a")]), sample(3.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Max,
        };
        let labels = HashMap::from([("pod".to_string(), "a".to_string())]);
        let filtered = series.filter(&labels);
        assert_eq!(filtered.aggregate_by, Aggregate::Max);
    }

    #[test]
    fn sample_builder_methods() {
        let s = MetricSample::new(1.0)
            .with_label("pod", "a")
            .with_timestamp(123.0);
        assert_eq!(s.labels["pod"], "a");
        assert_eq!(s.timestamp, Some(123.0));
    }

    #[test]
    fn scalar_is_alias_for_new() {
        assert_eq!(MetricSample::scalar(5.0), MetricSample::new(5.0));
    }

    #[test]
    fn finite_sample_serializes_as_number() {
        let json = serde_json::to_string(&MetricSample::new(0.95)).unwrap();
        assert!(json.contains("\"value\":0.95"));
        let back: MetricSample = serde_json::from_str(&json).unwrap();
        assert_eq!(back.value, 0.95);
    }

    #[test]
    fn non_finite_samples_round_trip() {
        for value in [f64::INFINITY, f64::NEG_INFINITY] {
            let json = serde_json::to_string(&MetricSample::new(value)).unwrap();
            assert!(
                !json.contains("\"value\":null"),
                "non-finite value must not serialize as null"
            );
            let back: MetricSample = serde_json::from_str(&json).unwrap();
            assert_eq!(back.value, value);
        }
        let json = serde_json::to_string(&MetricSample::new(f64::NAN)).unwrap();
        let back: MetricSample = serde_json::from_str(&json).unwrap();
        assert!(back.value.is_nan());
    }
}
