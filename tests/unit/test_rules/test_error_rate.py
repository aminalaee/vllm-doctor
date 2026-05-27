import pytest

from tests.helpers import snapshot_from_fixture
from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.error_rate import ErrorRateRule


@pytest.fixture
def rule() -> ErrorRateRule:
    return ErrorRateRule()


def _snapshot(success: float, errors: float, aborts: float) -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(
            request_success_total=success,
            request_error_total=errors,
            request_abort_total=aborts,
        ),
    )


class TestErrorRateRule:
    def test_no_finding_when_metrics_missing(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_when_total_zero(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(_snapshot(0, 0, 0)) == []

    def test_no_finding_below_threshold(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(_snapshot(100, 1, 2)) == []

    def test_finding_on_high_error_rate(self, rule: ErrorRateRule) -> None:
        findings = rule.evaluate(_snapshot(90, 10, 0))
        assert len(findings) == 1
        assert findings[0].severity == Severity.critical

    def test_finding_on_high_abort_rate(self, rule: ErrorRateRule) -> None:
        findings = rule.evaluate(_snapshot(80, 0, 20))
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_low_confidence_errors_only(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(_snapshot(90, 10, 0))[0].confidence == Confidence.low

    def test_low_confidence_aborts_only(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(_snapshot(80, 0, 20))[0].confidence == Confidence.low

    def test_high_confidence_both(self, rule: ErrorRateRule) -> None:
        assert rule.evaluate(_snapshot(70, 10, 20))[0].confidence == Confidence.high

    def test_evidence_contains_error_rate(self, rule: ErrorRateRule) -> None:
        findings = rule.evaluate(_snapshot(90, 10, 0))
        assert any("10.0%" in e for e in findings[0].evidence)

    def test_evidence_contains_abort_rate(self, rule: ErrorRateRule) -> None:
        findings = rule.evaluate(_snapshot(80, 0, 20))
        assert any("20.0%" in e for e in findings[0].evidence)

    def test_custom_thresholds(self) -> None:
        rule = ErrorRateRule(high_error_rate=0.20, high_abort_rate=0.30)
        assert rule.evaluate(_snapshot(90, 10, 0)) == []
        assert len(rule.evaluate(_snapshot(70, 30, 0))) == 1

    async def test_error_rate_with_fixture(self, rule: ErrorRateRule) -> None:
        snapshot = await snapshot_from_fixture("error-rate.txt")
        assert len(rule.evaluate(snapshot)) == 1

    def test_only_error_metric_present(self, rule: ErrorRateRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(request_error_total=10.0, request_success_total=90.0),
        )
        assert len(rule.evaluate(snapshot)) == 1
