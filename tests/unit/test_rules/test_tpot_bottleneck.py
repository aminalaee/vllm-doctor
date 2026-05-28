import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, Severity
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule

_CTX = DiagnosisContext(window="now")


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
        assert rule.evaluate(_CTX, Metrics()) == []

    def test_no_finding_below_threshold(self, rule: TPOTBottleneckRule) -> None:
        assert rule.evaluate(_CTX, Metrics(tpot_p95_seconds=0.1)) == []

    def test_no_finding_at_boundary(self, rule: TPOTBottleneckRule) -> None:
        assert rule.evaluate(_CTX, Metrics(tpot_p95_seconds=0.19)) == []

    def test_finding_at_threshold(self, rule: TPOTBottleneckRule) -> None:
        findings = rule.evaluate(_CTX, Metrics(tpot_p95_seconds=0.2))
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_low_confidence_tpot_only(self, rule: TPOTBottleneckRule, high_tpot: Metrics) -> None:
        assert rule.evaluate(_CTX, high_tpot)[0].confidence == Confidence.low

    def test_medium_confidence_with_low_gen(self, rule: TPOTBottleneckRule, high_tpot_low_gen: Metrics) -> None:
        assert rule.evaluate(_CTX, high_tpot_low_gen)[0].confidence == Confidence.medium

    def test_high_confidence_with_all_signals(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_normal_ttft: Metrics
    ) -> None:
        assert rule.evaluate(_CTX, high_tpot_low_gen_normal_ttft)[0].confidence == Confidence.high

    def test_high_gen_does_not_boost_confidence(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=200.0)
        assert rule.evaluate(_CTX, current)[0].confidence == Confidence.low

    def test_high_ttft_does_not_contribute_ttft_normal_signal(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=5.0)
        findings = rule.evaluate(_CTX, current)
        assert len(findings) == 1
        assert not any("TTFT p95 is normal" in s for s in findings[0].signals)

    def test_evidence_contains_tpot(self, rule: TPOTBottleneckRule, high_tpot: Metrics) -> None:
        findings = rule.evaluate(_CTX, high_tpot)
        assert any("0.500" in e for e in findings[0].evidence)

    def test_evidence_contains_gen_when_present(self, rule: TPOTBottleneckRule, high_tpot_low_gen: Metrics) -> None:
        findings = rule.evaluate(_CTX, high_tpot_low_gen)
        assert any("Generation" in e for e in findings[0].evidence)

    def test_evidence_contains_ttft_when_present(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_normal_ttft: Metrics
    ) -> None:
        findings = rule.evaluate(_CTX, high_tpot_low_gen_normal_ttft)
        assert any("TTFT" in e for e in findings[0].evidence)

    def test_no_finding_when_tpot_is_nan(self, rule: TPOTBottleneckRule) -> None:
        assert rule.evaluate(_CTX, Metrics(tpot_p95_seconds=float("nan"))) == []

    def test_nan_ttft_does_not_count_as_normal(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=20.0, ttft_p95_seconds=float("nan"))
        assert rule.evaluate(_CTX, current)[0].confidence == Confidence.medium

    def test_nan_ttft_not_in_evidence(self, rule: TPOTBottleneckRule) -> None:
        current = Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=float("nan"))
        findings = rule.evaluate(_CTX, current)
        assert not any("nan" in e for e in findings[0].evidence)

    async def test_tpot_bottleneck_with_scrape_fixture(self, rule: TPOTBottleneckRule) -> None:
        current = await snapshot_from_scrape_fixture("tpot-bottleneck.txt")
        # tpot_p95_seconds unavailable from scrape endpoint
        assert rule.evaluate(_CTX, current) == []

    async def test_tpot_bottleneck_with_prometheus_fixture(self, rule: TPOTBottleneckRule) -> None:
        current = await snapshot_from_prometheus_fixture("tpot-bottleneck.json")
        assert len(rule.evaluate(_CTX, current)) == 1

    def test_custom_threshold(self) -> None:
        rule = TPOTBottleneckRule(high_tpot_p95=0.5)
        assert rule.evaluate(_CTX, Metrics(tpot_p95_seconds=0.49)) == []
        assert len(rule.evaluate(_CTX, Metrics(tpot_p95_seconds=0.5))) == 1
