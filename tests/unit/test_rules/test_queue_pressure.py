import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, Metrics, Severity
from vllm_doctor.rules.queue_pressure import QueuePressureRule


@pytest.fixture
def rule() -> QueuePressureRule:
    return QueuePressureRule(high_waiting=5, high_running=50)


@pytest.fixture
def healthy() -> Metrics:
    return Metrics(num_requests_waiting=1, num_requests_running=10)


@pytest.fixture
def high_waiting() -> Metrics:
    return Metrics(num_requests_waiting=20, num_requests_running=10)


@pytest.fixture
def saturated() -> Metrics:
    return Metrics(num_requests_waiting=20, num_requests_running=80)


class TestQueuePressureRule:
    def test_no_finding_when_healthy(self, rule: QueuePressureRule, healthy: Metrics) -> None:
        assert rule.run(healthy) is None

    def test_finding_when_waiting_high(self, rule: QueuePressureRule, high_waiting: Metrics) -> None:
        result = rule.run(high_waiting)
        assert result is not None
        assert rule.severity == Severity.warning
        assert result.confidence == Confidence.low

    def test_high_confidence_when_both_signals(self, rule: QueuePressureRule, saturated: Metrics) -> None:
        result = rule.run(saturated)
        assert result is not None
        assert result.confidence == Confidence.high

    def test_no_finding_when_metrics_missing(self, rule: QueuePressureRule) -> None:
        assert rule.run(Metrics()) is None

    def test_evidence_contains_values(self, rule: QueuePressureRule, saturated: Metrics) -> None:
        result = rule.run(saturated)
        assert any("20" in e for e in result.evidence)
        assert any("80" in e for e in result.evidence)

    def test_no_finding_when_only_running_high(self, rule: QueuePressureRule) -> None:
        assert rule.run(Metrics(num_requests_waiting=1, num_requests_running=80)) is None

    async def test_queue_pressure_with_scrape_fixture(self) -> None:
        metrics = await snapshot_from_scrape_fixture("queue-pressure.txt")
        assert QueuePressureRule().run(metrics) is not None

    async def test_queue_pressure_with_prometheus_fixture(self) -> None:
        metrics = await snapshot_from_prometheus_fixture("queue-pressure.json")
        assert QueuePressureRule().run(metrics) is not None

    def test_custom_thresholds(self) -> None:
        rule = QueuePressureRule(high_waiting=100, high_running=200)
        assert rule.run(Metrics(num_requests_waiting=20, num_requests_running=80)) is None
