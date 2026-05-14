import pytest

from vllm_doctor.models import Confidence, MetricSnapshot, Severity
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule


@pytest.fixture
def rule() -> KVCachePressureRule:
    return KVCachePressureRule()


@pytest.fixture
def high_cache_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="now", gpu_cache_usage_perc=0.95)


@pytest.fixture
def high_cache_with_waiting_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now", gpu_cache_usage_perc=0.95, num_requests_waiting=5
    )


class TestKVCachePressureRule:
    def test_no_finding_when_metric_missing(self, rule: KVCachePressureRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_below_threshold(self, rule: KVCachePressureRule) -> None:
        assert (
            rule.evaluate(MetricSnapshot(window="now", gpu_cache_usage_perc=0.80)) == []
        )

    def test_no_finding_at_threshold_boundary(self, rule: KVCachePressureRule) -> None:
        assert (
            rule.evaluate(MetricSnapshot(window="now", gpu_cache_usage_perc=0.89)) == []
        )

    def test_finding_at_threshold(self, rule: KVCachePressureRule) -> None:
        findings = rule.evaluate(
            MetricSnapshot(window="now", gpu_cache_usage_perc=0.90)
        )
        assert len(findings) == 1
        assert findings[0].severity == Severity.critical

    def test_medium_confidence_without_waiting(
        self, rule: KVCachePressureRule, high_cache_snapshot: MetricSnapshot
    ) -> None:
        assert rule.evaluate(high_cache_snapshot)[0].confidence == Confidence.medium

    def test_high_confidence_with_waiting(
        self,
        rule: KVCachePressureRule,
        high_cache_with_waiting_snapshot: MetricSnapshot,
    ) -> None:
        assert (
            rule.evaluate(high_cache_with_waiting_snapshot)[0].confidence
            == Confidence.high
        )

    def test_waiting_zero_gives_medium_confidence(
        self, rule: KVCachePressureRule
    ) -> None:
        snapshot = MetricSnapshot(
            window="now", gpu_cache_usage_perc=0.95, num_requests_waiting=0
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.medium

    def test_evidence_contains_cache_usage(
        self, rule: KVCachePressureRule, high_cache_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(high_cache_snapshot)
        assert any("95%" in e for e in findings[0].evidence)

    def test_evidence_contains_waiting_when_present(
        self,
        rule: KVCachePressureRule,
        high_cache_with_waiting_snapshot: MetricSnapshot,
    ) -> None:
        findings = rule.evaluate(high_cache_with_waiting_snapshot)
        assert any("5" in e for e in findings[0].evidence)

    def test_custom_threshold(self) -> None:
        rule = KVCachePressureRule(high_cache_usage=0.75)
        assert (
            rule.evaluate(MetricSnapshot(window="now", gpu_cache_usage_perc=0.74)) == []
        )
        assert (
            len(rule.evaluate(MetricSnapshot(window="now", gpu_cache_usage_perc=0.75)))
            == 1
        )
