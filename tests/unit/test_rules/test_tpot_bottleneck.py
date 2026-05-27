import pytest

from tests.helpers import snapshot_from_fixture
from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule


@pytest.fixture
def rule() -> TPOTBottleneckRule:
    return TPOTBottleneckRule()


@pytest.fixture
def high_tpot_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.5))


@pytest.fixture
def high_tpot_low_gen_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=20.0),
    )


@pytest.fixture
def high_tpot_low_gen_normal_ttft_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(
            tpot_p95_seconds=0.5,
            generation_tokens_per_second=20.0,
            ttft_p95_seconds=0.3,
        ),
    )


class TestTPOTBottleneckRule:
    def test_no_finding_when_metric_missing(self, rule: TPOTBottleneckRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_below_threshold(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.1))
        assert rule.evaluate(snapshot) == []

    def test_no_finding_at_boundary(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.19))
        assert rule.evaluate(snapshot) == []

    def test_finding_at_threshold(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.2))
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_low_confidence_tpot_only(
        self, rule: TPOTBottleneckRule, high_tpot_snapshot: MetricSnapshot
    ) -> None:
        assert rule.evaluate(high_tpot_snapshot)[0].confidence == Confidence.low

    def test_medium_confidence_with_low_gen(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_snapshot: MetricSnapshot
    ) -> None:
        assert (
            rule.evaluate(high_tpot_low_gen_snapshot)[0].confidence == Confidence.medium
        )

    def test_high_confidence_with_all_signals(
        self,
        rule: TPOTBottleneckRule,
        high_tpot_low_gen_normal_ttft_snapshot: MetricSnapshot,
    ) -> None:
        assert (
            rule.evaluate(high_tpot_low_gen_normal_ttft_snapshot)[0].confidence
            == Confidence.high
        )

    def test_high_gen_does_not_boost_confidence(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(tpot_p95_seconds=0.5, generation_tokens_per_second=200.0),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.low

    def test_high_ttft_does_not_contribute_ttft_normal_signal(
        self, rule: TPOTBottleneckRule
    ) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=5.0),
        )
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert not any("TTFT p95 is normal" in s for s in findings[0].signals)

    def test_evidence_contains_tpot(
        self, rule: TPOTBottleneckRule, high_tpot_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(high_tpot_snapshot)
        assert any("0.500" in e for e in findings[0].evidence)

    def test_evidence_contains_gen_when_present(
        self, rule: TPOTBottleneckRule, high_tpot_low_gen_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(high_tpot_low_gen_snapshot)
        assert any("Generation" in e for e in findings[0].evidence)

    def test_evidence_contains_ttft_when_present(
        self,
        rule: TPOTBottleneckRule,
        high_tpot_low_gen_normal_ttft_snapshot: MetricSnapshot,
    ) -> None:
        findings = rule.evaluate(high_tpot_low_gen_normal_ttft_snapshot)
        assert any("TTFT" in e for e in findings[0].evidence)

    def test_no_finding_when_tpot_is_nan(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(tpot_p95_seconds=float("nan"))
        )
        assert rule.evaluate(snapshot) == []

    def test_nan_ttft_does_not_count_as_normal(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(
                tpot_p95_seconds=0.5,
                generation_tokens_per_second=20.0,
                ttft_p95_seconds=float("nan"),
            ),
        )
        findings = rule.evaluate(snapshot)
        assert findings[0].confidence == Confidence.medium

    def test_nan_ttft_not_in_evidence(self, rule: TPOTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(tpot_p95_seconds=0.5, ttft_p95_seconds=float("nan")),
        )
        findings = rule.evaluate(snapshot)
        assert not any("nan" in e for e in findings[0].evidence)

    async def test_tpot_bottleneck_with_fixture(self, rule: TPOTBottleneckRule) -> None:
        snapshot = await snapshot_from_fixture("tpot-bottleneck.txt")
        # tpot_p95_seconds unavailable from scrape endpoint
        assert rule.evaluate(snapshot) == []

    def test_custom_threshold(self) -> None:
        rule = TPOTBottleneckRule(high_tpot_p95=0.5)
        assert (
            rule.evaluate(
                MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.49))
            )
            == []
        )
        assert (
            len(
                rule.evaluate(
                    MetricSnapshot(window="now", metrics=Metrics(tpot_p95_seconds=0.5))
                )
            )
            == 1
        )
