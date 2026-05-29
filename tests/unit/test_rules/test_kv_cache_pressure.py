import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, Severity
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule

_CTX = DiagnosisContext(window="now")


@pytest.fixture
def rule() -> KVCachePressureRule:
    return KVCachePressureRule()


@pytest.fixture
def high_cache() -> Metrics:
    return Metrics(kv_cache_usage_perc=0.95)


@pytest.fixture
def high_cache_with_waiting() -> Metrics:
    return Metrics(kv_cache_usage_perc=0.95, num_requests_waiting=5)


class TestKVCachePressureRule:
    def test_no_finding_when_metric_missing(self, rule: KVCachePressureRule) -> None:
        assert rule.run(_CTX, Metrics()) == []

    def test_no_finding_below_threshold(self, rule: KVCachePressureRule) -> None:
        assert rule.run(_CTX, Metrics(kv_cache_usage_perc=0.80)) == []

    def test_no_finding_at_threshold_boundary(self, rule: KVCachePressureRule) -> None:
        assert rule.run(_CTX, Metrics(kv_cache_usage_perc=0.89)) == []

    def test_finding_at_threshold(self, rule: KVCachePressureRule) -> None:
        findings = rule.run(_CTX, Metrics(kv_cache_usage_perc=0.90))
        assert len(findings) == 1
        assert findings[0].severity == Severity.critical

    def test_medium_confidence_without_waiting(self, rule: KVCachePressureRule, high_cache: Metrics) -> None:
        assert rule.run(_CTX, high_cache)[0].confidence == Confidence.medium

    def test_high_confidence_with_waiting(self, rule: KVCachePressureRule, high_cache_with_waiting: Metrics) -> None:
        assert rule.run(_CTX, high_cache_with_waiting)[0].confidence == Confidence.high

    def test_waiting_zero_gives_medium_confidence(self, rule: KVCachePressureRule) -> None:
        current = Metrics(kv_cache_usage_perc=0.95, num_requests_waiting=0)
        assert rule.run(_CTX, current)[0].confidence == Confidence.medium

    def test_evidence_contains_cache_usage(self, rule: KVCachePressureRule, high_cache: Metrics) -> None:
        findings = rule.run(_CTX, high_cache)
        assert any("95%" in e for e in findings[0].evidence)

    def test_evidence_contains_waiting_when_present(
        self, rule: KVCachePressureRule, high_cache_with_waiting: Metrics
    ) -> None:
        findings = rule.run(_CTX, high_cache_with_waiting)
        assert any("5" in e for e in findings[0].evidence)

    async def test_kv_cache_pressure_with_scrape_fixture(self, rule: KVCachePressureRule) -> None:
        current = await snapshot_from_scrape_fixture("kv-pressure.txt")
        assert len(rule.run(_CTX, current)) == 1

    async def test_kv_cache_pressure_with_prometheus_fixture(self, rule: KVCachePressureRule) -> None:
        current = await snapshot_from_prometheus_fixture("kv-pressure.json")
        assert len(rule.run(_CTX, current)) == 1

    def test_custom_threshold(self) -> None:
        rule = KVCachePressureRule(high_cache_usage=0.75)
        assert rule.run(_CTX, Metrics(kv_cache_usage_perc=0.74)) == []
        assert len(rule.run(_CTX, Metrics(kv_cache_usage_perc=0.75))) == 1
