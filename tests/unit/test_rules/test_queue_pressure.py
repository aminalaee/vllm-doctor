import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, Severity
from vllm_doctor.rules.queue_pressure import QueuePressureRule

_CTX = DiagnosisContext(window="1h")


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
        assert rule.run(_CTX, healthy) == []

    def test_finding_when_waiting_high(self, rule: QueuePressureRule, high_waiting: Metrics) -> None:
        findings = rule.run(_CTX, high_waiting)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning
        assert findings[0].confidence == Confidence.low

    def test_high_confidence_when_both_signals(self, rule: QueuePressureRule, saturated: Metrics) -> None:
        findings = rule.run(_CTX, saturated)
        assert len(findings) == 1
        assert findings[0].confidence == Confidence.high

    def test_no_finding_when_metrics_missing(self, rule: QueuePressureRule) -> None:
        assert rule.run(_CTX, Metrics()) == []

    def test_evidence_contains_values(self, rule: QueuePressureRule, saturated: Metrics) -> None:
        findings = rule.run(_CTX, saturated)
        assert any("20" in e for e in findings[0].evidence)
        assert any("80" in e for e in findings[0].evidence)

    def test_no_finding_when_only_running_high(self, rule: QueuePressureRule) -> None:
        assert rule.run(_CTX, Metrics(num_requests_waiting=1, num_requests_running=80)) == []

    async def test_queue_pressure_with_scrape_fixture(self) -> None:
        current = await snapshot_from_scrape_fixture("queue-pressure.txt")
        assert len(QueuePressureRule().run(_CTX, current)) == 1

    async def test_queue_pressure_with_prometheus_fixture(self) -> None:
        current = await snapshot_from_prometheus_fixture("queue-pressure.json")
        assert len(QueuePressureRule().run(_CTX, current)) == 1

    def test_custom_thresholds(self) -> None:
        rule = QueuePressureRule(high_waiting=100, high_running=200)
        assert rule.run(_CTX, Metrics(num_requests_waiting=20, num_requests_running=80)) == []
