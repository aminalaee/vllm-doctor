import pytest

from tests.helpers import snapshot_from_fixture
from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.queue_pressure import QueuePressureRule


@pytest.fixture
def rule() -> QueuePressureRule:
    return QueuePressureRule(high_waiting=5, high_running=50)


@pytest.fixture
def healthy_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="1h", metrics=Metrics(num_requests_waiting=1, num_requests_running=10))


@pytest.fixture
def high_waiting_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="1h", metrics=Metrics(num_requests_waiting=20, num_requests_running=10))


@pytest.fixture
def saturated_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="1h", metrics=Metrics(num_requests_waiting=20, num_requests_running=80))


class TestQueuePressureRule:
    def test_no_finding_when_healthy(self, rule: QueuePressureRule, healthy_snapshot: MetricSnapshot) -> None:
        assert rule.evaluate(healthy_snapshot) == []

    def test_finding_when_waiting_high(self, rule: QueuePressureRule, high_waiting_snapshot: MetricSnapshot) -> None:
        findings = rule.evaluate(high_waiting_snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning
        assert findings[0].confidence == Confidence.low

    def test_high_confidence_when_both_signals(
        self, rule: QueuePressureRule, saturated_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(saturated_snapshot)
        assert len(findings) == 1
        assert findings[0].confidence == Confidence.high

    def test_no_finding_when_metrics_missing(self, rule: QueuePressureRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="1h")) == []

    def test_evidence_contains_values(self, rule: QueuePressureRule, saturated_snapshot: MetricSnapshot) -> None:
        findings = rule.evaluate(saturated_snapshot)
        assert any("20" in e for e in findings[0].evidence)
        assert any("80" in e for e in findings[0].evidence)

    def test_no_finding_when_only_running_high(self, rule: QueuePressureRule) -> None:
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(num_requests_waiting=1, num_requests_running=80),
        )
        assert rule.evaluate(snapshot) == []

    async def test_queue_pressure_with_fixture(self) -> None:
        snapshot = await snapshot_from_fixture("queue-pressure.txt")
        assert len(QueuePressureRule().evaluate(snapshot)) == 1

    def test_custom_thresholds(self) -> None:
        rule = QueuePressureRule(high_waiting=100, high_running=200)
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(num_requests_waiting=20, num_requests_running=80),
        )
        assert rule.evaluate(snapshot) == []
