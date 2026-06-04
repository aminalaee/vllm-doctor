from vllm_doctor.clients.models import MetricSample
from vllm_doctor.metrics import (
    METRIC_SPECS,
    METRIC_SPECS_BY_OUTPUT,
    Aggregate,
    Metrics,
    MetricSeries,
    MetricSeriesSnapshot,
)
from vllm_doctor.probes import PROBES


def test_metric_registry_matches_models() -> None:
    metric_fields = {spec.output for spec in METRIC_SPECS}

    assert metric_fields == set(Metrics.model_fields)
    assert metric_fields == set(MetricSeriesSnapshot.model_fields)


def test_metric_registry_has_unique_outputs() -> None:
    outputs = [spec.output for spec in METRIC_SPECS]

    assert len(outputs) == len(set(outputs))
    assert set(METRIC_SPECS_BY_OUTPUT) == set(outputs)


def test_metric_registry_has_display_metadata() -> None:
    for spec in METRIC_SPECS:
        assert spec.display.title
        assert spec.display.fmt


def test_metric_registry_references_known_probes() -> None:
    probe_names = set(PROBES)

    for spec in METRIC_SPECS:
        assert spec.probe_names() <= probe_names


def test_snapshot_applies_spec_aggregation_to_directly_constructed_series() -> None:
    snapshot = MetricSeriesSnapshot(
        kv_cache_usage_perc=MetricSeries(
            samples=[
                MetricSample(labels={"pod": "a", "engine": "0"}, value=0.5),
                MetricSample(labels={"pod": "a", "engine": "1"}, value=0.9),
                MetricSample(labels={"pod": "b"}, value=0.3),
            ]
        )
    )

    assert snapshot.kv_cache_usage_perc.aggregate_by == Aggregate.max
    assert snapshot.kv_cache_usage_perc.by("pod") == {"a": 0.9, "b": 0.3}
    assert snapshot.to_metrics().kv_cache_usage_perc == 0.9
