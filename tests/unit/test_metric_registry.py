from vllm_doctor.metrics import METRIC_SPECS, METRIC_SPECS_BY_OUTPUT, Metrics, MetricSeriesSnapshot
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
