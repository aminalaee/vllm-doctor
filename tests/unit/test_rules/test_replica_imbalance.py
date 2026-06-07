import pytest

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.metrics import MetricSeries, MetricSeriesSnapshot
from vllm_doctor.models import Confidence, Severity
from vllm_doctor.rules.replica_imbalance import ReplicaImbalanceRule


def _series(*samples: tuple[dict[str, str], float]) -> MetricSeries:
    return MetricSeries(samples=[MetricSample(labels=labels, value=value) for labels, value in samples])


def _pod(name: str, model: str | None = "m") -> dict[str, str]:
    labels = {"pod": name}
    if model is not None:
        labels["model_name"] = model
    return labels


@pytest.fixture
def rule() -> ReplicaImbalanceRule:
    return ReplicaImbalanceRule()


class TestReplicaImbalanceRule:
    def test_no_finding_single_replica(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(num_requests_running=_series((_pod("vllm-0"), 40.0)))
        assert rule.run(snapshot) is None

    def test_no_finding_when_balanced(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series((_pod("vllm-0"), 10.0), (_pod("vllm-1"), 12.0)),
            kv_cache_usage_perc=_series((_pod("vllm-0"), 0.50), (_pod("vllm-1"), 0.55)),
        )
        assert rule.run(snapshot) is None

    def test_no_finding_when_below_min_total_running(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series((_pod("vllm-0"), 3.0), (_pod("vllm-1"), 0.0)),
        )
        assert rule.run(snapshot) is None

    def test_finding_on_running_spread(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series((_pod("vllm-0"), 50.0), (_pod("vllm-1"), 4.0)),
        )
        result = rule.run(snapshot)
        assert result is not None
        assert rule.severity == Severity.warning
        assert result.confidence == Confidence.low
        assert any("vllm-0" in e and "vllm-1" in e for e in result.evidence)

    def test_finding_on_cache_gap(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            kv_cache_usage_perc=_series((_pod("vllm-0"), 0.92), (_pod("vllm-1"), 0.10)),
        )
        result = rule.run(snapshot)
        assert result is not None
        assert result.confidence == Confidence.low

    def test_high_confidence_when_three_signals(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series((_pod("vllm-0"), 50.0), (_pod("vllm-1"), 4.0)),
            num_requests_waiting=_series((_pod("vllm-0"), 7.0), (_pod("vllm-1"), 0.0)),
            kv_cache_usage_perc=_series((_pod("vllm-0"), 0.92), (_pod("vllm-1"), 0.10)),
        )
        result = rule.run(snapshot)
        assert result is not None
        assert result.confidence == Confidence.high

    def test_groups_by_model_only_reports_imbalanced(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series(
                (_pod("a-0", "balanced"), 10.0),
                (_pod("a-1", "balanced"), 11.0),
                (_pod("b-0", "skewed"), 50.0),
                (_pod("b-1", "skewed"), 3.0),
            ),
        )
        result = rule.run(snapshot)
        assert result is not None
        assert len(result.evidence) == 1
        assert "skewed" in result.evidence[0]
        assert "balanced" not in result.evidence[0]

    def test_does_not_compare_across_models(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series(
                (_pod("a-0", "llama-70b"), 50.0),
                (_pod("b-0", "qwen-0.5b"), 3.0),
            ),
        )
        assert rule.run(snapshot) is None

    def test_fires_without_model_label(self, rule: ReplicaImbalanceRule) -> None:
        snapshot = MetricSeriesSnapshot(
            num_requests_running=_series((_pod("vllm-0", None), 50.0), (_pod("vllm-1", None), 4.0)),
        )
        result = rule.run(snapshot)
        assert result is not None
        assert all(": " not in e or e.startswith("running") for e in result.evidence)
