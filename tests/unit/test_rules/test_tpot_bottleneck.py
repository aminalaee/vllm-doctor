import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, Metrics, Severity
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule


@pytest.fixture
def rule() -> TPOTBottleneckRule:
    return TPOTBottleneckRule()


@pytest.fixture
def high_tpot() -> Metrics:
    return Metrics(tpot_p95_seconds=0.5)


@pytest.fixture
def high_tpot_low_gen() -> Metrics:
    return Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=20.0)


@pytest.fixture
def high_tpot_low_gen_normal_ttft() -> Metrics:
    return Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=20.0, ttft_p95_seconds=0.3)


class TestTPOTBottleneckRule:
    def test_no_finding_when_metric_missing(self, rule: TPOTBottleneckRule) -> None:
        assert rule.run(Metrics()) is None

    def test_no_finding_below_threshold(self, rule: TPOTBottleneckRule) -> None:
        assert rule.run(Metrics(tpot_p95_seconds=0.1)) is None

    def test_no_finding_at_boundary(self, rule: TPOTBottleneckRule) -> None:
        assert rule.run(Metrics(tpot_p95_seconds=0.19)) is None

    def test_finding_at_threshold(self, rule: TPOTBottleneckRule) -> None:
        result = rule.run(Metrics(tpot_p95_seconds=0.2))
        assert result is not None
        assert rule.severity == Severity.warning

    def test_low_confidence_tpot_only(self, rule: TPOTBottleneckRule, high_tpot: Metrics) -> None:
        assert rule.run(high_tpot).confidence == Confidence.low

    def test_medium_confidence_with_low_gen(self, rule: TPOTBottleneckRule, high_tpot_low_gen: Metrics) -> None:
        assert rule.run(high_tpot_low_gen).confidence == Confidence.medium

    def test_high_confidence_with_all_signals(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_normal_ttft: Metrics
    ) -> None:
        assert rule.run(high_tpot_low_gen_normal_ttft).confidence == Confidence.high

    def test_high_gen_does_not_boost_confidence(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=200.0)
        assert rule.run(current).confidence == Confidence.low

    def test_high_ttft_does_not_contribute_ttft_normal_signal(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=5.0)
        result = rule.run(current)
        assert result is not None
        assert not any("TTFT p95 is normal" in s for s in result.signals)

    def test_evidence_contains_tpot(self, rule: TPOTBottleneckRule, high_tpot: Metrics) -> None:
        result = rule.run(high_tpot)
        assert any("0.500" in e for e in result.evidence)

    def test_evidence_contains_gen_when_present(self, rule: TPOTBottleneckRule, high_tpot_low_gen: Metrics) -> None:
        result = rule.run(high_tpot_low_gen)
        assert any("Generation" in e for e in result.evidence)

    def test_evidence_contains_ttft_when_present(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_normal_ttft: Metrics
    ) -> None:
        result = rule.run(high_tpot_low_gen_normal_ttft)
        assert any("TTFT" in e for e in result.evidence)

    def test_no_finding_when_tpot_is_nan(self, rule: TPOTBottleneckRule) -> None:
        assert rule.run(Metrics(tpot_p95_seconds=float("nan"))) is None

    def test_nan_ttft_does_not_count_as_normal(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=20.0, ttft_p95_seconds=float("nan"))
        assert rule.run(current).confidence == Confidence.medium

    def test_nan_ttft_not_in_evidence(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=float("nan"))
        result = rule.run(current)
        assert not any("nan" in e for e in result.evidence)

    async def test_tpot_bottleneck_with_scrape_fixture(self, rule: TPOTBottleneckRule) -> None:
        current = await snapshot_from_scrape_fixture("tpot-bottleneck.txt")
        # tpot_p95_seconds unavailable from scrape endpoint
        assert rule.run(current) is None

    async def test_tpot_bottleneck_with_prometheus_fixture(self, rule: TPOTBottleneckRule) -> None:
        current = await snapshot_from_prometheus_fixture("tpot-bottleneck.json")
        assert rule.run(current) is not None

    def test_custom_threshold(self) -> None:
        rule = TPOTBottleneckRule(high_tpot_p95=0.5)
        assert rule.run(Metrics(tpot_p95_seconds=0.49)) is None
        assert rule.run(Metrics(tpot_p95_seconds=0.5)) is not None
