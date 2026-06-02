import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, Severity
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule


@pytest.fixture
def rule() -> TTFTBottleneckRule:
    return TTFTBottleneckRule()


@pytest.fixture
def high_ttft() -> MetricSeriesSnapshot:
    return MetricSeriesSnapshot(ttft_p95_seconds=3.0)


@pytest.fixture
def high_ttft_stable_tpot() -> MetricSeriesSnapshot:
    return MetricSeriesSnapshot(ttft_p95_seconds=3.0, tpot_p95_seconds=0.05)


@pytest.fixture
def high_ttft_stable_tpot_waiting() -> MetricSeriesSnapshot:
    return MetricSeriesSnapshot(ttft_p95_seconds=3.0, tpot_p95_seconds=0.05, num_requests_waiting=10)


class TestTTFTBottleneckRule:
    def test_no_finding_when_metric_missing(self, rule: TTFTBottleneckRule) -> None:
        assert rule.run(MetricSeriesSnapshot()) is None

    def test_no_finding_below_threshold(self, rule: TTFTBottleneckRule) -> None:
        assert rule.run(MetricSeriesSnapshot(ttft_p95_seconds=1.5)) is None

    def test_no_finding_at_boundary(self, rule: TTFTBottleneckRule) -> None:
        assert rule.run(MetricSeriesSnapshot(ttft_p95_seconds=1.99)) is None

    def test_finding_at_threshold(self, rule: TTFTBottleneckRule) -> None:
        result = rule.run(MetricSeriesSnapshot(ttft_p95_seconds=2.0))
        assert result is not None
        assert rule.severity == Severity.warning

    def test_low_confidence_ttft_only(self, rule: TTFTBottleneckRule, high_ttft: MetricSeriesSnapshot) -> None:
        assert rule.run(high_ttft).confidence == Confidence.low

    def test_medium_confidence_with_stable_tpot(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot: MetricSeriesSnapshot
    ) -> None:
        assert rule.run(high_ttft_stable_tpot).confidence == Confidence.medium

    def test_high_confidence_with_all_signals(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot_waiting: MetricSeriesSnapshot
    ) -> None:
        assert rule.run(high_ttft_stable_tpot_waiting).confidence == Confidence.high

    def test_evidence_contains_ttft(self, rule: TTFTBottleneckRule, high_ttft: MetricSeriesSnapshot) -> None:
        result = rule.run(high_ttft)
        assert any("3.000" in e for e in result.evidence)

    def test_evidence_contains_tpot_when_present(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot: MetricSeriesSnapshot
    ) -> None:
        result = rule.run(high_ttft_stable_tpot)
        assert any("TPOT" in e for e in result.evidence)

    def test_evidence_contains_waiting_when_present(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot_waiting: MetricSeriesSnapshot
    ) -> None:
        result = rule.run(high_ttft_stable_tpot_waiting)
        assert any("10" in e for e in result.evidence)

    def test_high_tpot_does_not_boost_confidence(self, rule: TTFTBottleneckRule) -> None:
        metrics = MetricSeriesSnapshot(ttft_p95_seconds=3.0, tpot_p95_seconds=0.5)
        assert rule.run(metrics).confidence == Confidence.low

    def test_no_finding_when_ttft_is_nan(self, rule: TTFTBottleneckRule) -> None:
        assert rule.run(MetricSeriesSnapshot(ttft_p95_seconds=float("nan"))) is None

    def test_no_finding_when_tpot_is_nan(self, rule: TTFTBottleneckRule) -> None:
        metrics = MetricSeriesSnapshot(ttft_p95_seconds=3.0, tpot_p95_seconds=float("nan"))
        result = rule.run(metrics)
        assert result is not None
        assert not any("nan" in e for e in result.evidence)

    async def test_ttft_bottleneck_with_scrape_fixture(self, rule: TTFTBottleneckRule) -> None:
        metrics = await snapshot_from_scrape_fixture("ttft-bottleneck.txt")
        # ttft_p95_seconds unavailable from scrape endpoint
        assert rule.run(metrics) is None

    async def test_ttft_bottleneck_with_prometheus_fixture(self, rule: TTFTBottleneckRule) -> None:
        metrics = await snapshot_from_prometheus_fixture("ttft-bottleneck.json")
        assert rule.run(metrics) is not None

    def test_custom_threshold(self) -> None:
        rule = TTFTBottleneckRule(high_ttft_p95=5.0)
        assert rule.run(MetricSeriesSnapshot(ttft_p95_seconds=4.9)) is None
        assert rule.run(MetricSeriesSnapshot(ttft_p95_seconds=5.0)) is not None
