import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, Metrics, Severity
from vllm_doctor.rules.error_rate import ErrorRateRule


@pytest.fixture
def rule() -> ErrorRateRule:
    return ErrorRateRule()


def _metrics(success: float, errors: float, aborts: float) -> Metrics:
    return Metrics(
        request_success_total=success,
        request_error_total=errors,
        request_abort_total=aborts,
    )


class TestErrorRateRule:
    def test_no_finding_when_metrics_missing(self, rule: ErrorRateRule) -> None:
        assert rule.run(Metrics()) is None

    def test_no_finding_when_total_zero(self, rule: ErrorRateRule) -> None:
        assert rule.run(_metrics(0, 0, 0)) is None

    def test_no_finding_below_threshold(self, rule: ErrorRateRule) -> None:
        assert rule.run(_metrics(100, 1, 2)) is None

    def test_finding_on_high_error_rate(self, rule: ErrorRateRule) -> None:
        result = rule.run(_metrics(90, 10, 0))
        assert result is not None
        assert result.severity == Severity.critical

    def test_finding_on_high_abort_rate(self, rule: ErrorRateRule) -> None:
        result = rule.run(_metrics(80, 0, 20))
        assert result is not None
        assert result.severity == Severity.warning

    def test_low_confidence_errors_only(self, rule: ErrorRateRule) -> None:
        assert rule.run(_metrics(90, 10, 0)).confidence == Confidence.low

    def test_low_confidence_aborts_only(self, rule: ErrorRateRule) -> None:
        assert rule.run(_metrics(80, 0, 20)).confidence == Confidence.low

    def test_high_confidence_both(self, rule: ErrorRateRule) -> None:
        assert rule.run(_metrics(70, 10, 20)).confidence == Confidence.high

    def test_evidence_contains_error_rate(self, rule: ErrorRateRule) -> None:
        result = rule.run(_metrics(90, 10, 0))
        assert any("10.0%" in e for e in result.evidence)

    def test_evidence_contains_abort_rate(self, rule: ErrorRateRule) -> None:
        result = rule.run(_metrics(80, 0, 20))
        assert any("20.0%" in e for e in result.evidence)

    def test_custom_thresholds(self) -> None:
        rule = ErrorRateRule(high_error_rate=0.20, high_abort_rate=0.30)
        assert rule.run(_metrics(90, 10, 0)) is None
        assert rule.run(_metrics(70, 30, 0)) is not None

    async def test_error_rate_with_scrape_fixture(self, rule: ErrorRateRule) -> None:
        current = await snapshot_from_scrape_fixture("error-rate.txt")
        assert rule.run(current) is not None

    async def test_error_rate_with_prometheus_fixture(self, rule: ErrorRateRule) -> None:
        current = await snapshot_from_prometheus_fixture("error-rate.json")
        assert rule.run(current) is not None

    def test_only_error_metric_present(self, rule: ErrorRateRule) -> None:
        current = Metrics(request_error_total=10.0, request_success_total=90.0)
        assert rule.run(current) is not None
