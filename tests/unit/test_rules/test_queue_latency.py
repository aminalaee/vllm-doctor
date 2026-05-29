import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, Severity
from vllm_doctor.rules.queue_latency import QueueLatencyRule

_CTX = DiagnosisContext(window="now")


@pytest.fixture
def rule() -> QueueLatencyRule:
    return QueueLatencyRule()


class TestQueueLatencyRule:
    def test_no_finding_when_metric_missing(self, rule: QueueLatencyRule) -> None:
        assert rule.run(_CTX, Metrics()) == []

    def test_no_finding_below_threshold(self, rule: QueueLatencyRule) -> None:
        assert rule.run(_CTX, Metrics(queue_time_p95_seconds=0.5)) == []

    def test_no_finding_at_boundary(self, rule: QueueLatencyRule) -> None:
        assert rule.run(_CTX, Metrics(queue_time_p95_seconds=0.99)) == []

    def test_finding_at_threshold(self, rule: QueueLatencyRule) -> None:
        findings = rule.run(_CTX, Metrics(queue_time_p95_seconds=1.0))
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_low_confidence_without_waiting(self, rule: QueueLatencyRule) -> None:
        assert rule.run(_CTX, Metrics(queue_time_p95_seconds=2.0))[0].confidence == Confidence.low

    def test_high_confidence_with_waiting(self, rule: QueueLatencyRule) -> None:
        current = Metrics(queue_time_p95_seconds=2.0, num_requests_waiting=5)
        assert rule.run(_CTX, current)[0].confidence == Confidence.high

    def test_low_confidence_when_waiting_is_zero(self, rule: QueueLatencyRule) -> None:
        current = Metrics(queue_time_p95_seconds=2.0, num_requests_waiting=0)
        assert rule.run(_CTX, current)[0].confidence == Confidence.low

    def test_no_finding_when_nan(self, rule: QueueLatencyRule) -> None:
        assert rule.run(_CTX, Metrics(queue_time_p95_seconds=float("nan"))) == []

    def test_evidence_contains_queue_time(self, rule: QueueLatencyRule) -> None:
        findings = rule.run(_CTX, Metrics(queue_time_p95_seconds=1.5))
        assert any("1.500" in e for e in findings[0].evidence)

    def test_evidence_contains_waiting_count_when_present(self, rule: QueueLatencyRule) -> None:
        current = Metrics(queue_time_p95_seconds=2.0, num_requests_waiting=8)
        findings = rule.run(_CTX, current)
        assert any("8" in e for e in findings[0].evidence)

    def test_summary_contains_queue_time(self, rule: QueueLatencyRule) -> None:
        assert "3.00" in rule.run(_CTX, Metrics(queue_time_p95_seconds=3.0))[0].summary

    async def test_queue_latency_with_scrape_fixture(self, rule: QueueLatencyRule) -> None:
        current = await snapshot_from_scrape_fixture("queue-latency.txt")
        # queue_time_p95_seconds unavailable from scrape endpoint
        assert rule.run(_CTX, current) == []

    async def test_queue_latency_with_prometheus_fixture(self, rule: QueueLatencyRule) -> None:
        current = await snapshot_from_prometheus_fixture("queue-latency.json")
        assert len(rule.run(_CTX, current)) == 1

    def test_custom_threshold(self) -> None:
        rule = QueueLatencyRule(high_queue_time_p95=5.0)
        assert rule.run(_CTX, Metrics(queue_time_p95_seconds=4.9)) == []
        assert len(rule.run(_CTX, Metrics(queue_time_p95_seconds=5.0))) == 1
