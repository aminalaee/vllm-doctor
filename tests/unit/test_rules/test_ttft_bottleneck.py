import pytest

from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule


@pytest.fixture
def rule() -> TTFTBottleneckRule:
    return TTFTBottleneckRule()


@pytest.fixture
def high_ttft_snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=3.0))


@pytest.fixture
def high_ttft_stable_tpot_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(ttft_p95_seconds=3.0, tpot_p95_seconds=0.05),
    )


@pytest.fixture
def high_ttft_stable_tpot_waiting_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(
            ttft_p95_seconds=3.0, tpot_p95_seconds=0.05, num_requests_waiting=10
        ),
    )


class TestTTFTBottleneckRule:
    def test_no_finding_when_metric_missing(self, rule: TTFTBottleneckRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_below_threshold(self, rule: TTFTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=1.5))
        assert rule.evaluate(snapshot) == []

    def test_no_finding_at_boundary(self, rule: TTFTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=1.99))
        assert rule.evaluate(snapshot) == []

    def test_finding_at_threshold(self, rule: TTFTBottleneckRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=2.0))
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_low_confidence_ttft_only(
        self, rule: TTFTBottleneckRule, high_ttft_snapshot: MetricSnapshot
    ) -> None:
        assert rule.evaluate(high_ttft_snapshot)[0].confidence == Confidence.low

    def test_medium_confidence_with_stable_tpot(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot_snapshot: MetricSnapshot
    ) -> None:
        assert (
            rule.evaluate(high_ttft_stable_tpot_snapshot)[0].confidence
            == Confidence.medium
        )

    def test_high_confidence_with_all_signals(
        self,
        rule: TTFTBottleneckRule,
        high_ttft_stable_tpot_waiting_snapshot: MetricSnapshot,
    ) -> None:
        assert (
            rule.evaluate(high_ttft_stable_tpot_waiting_snapshot)[0].confidence
            == Confidence.high
        )

    def test_evidence_contains_ttft(
        self, rule: TTFTBottleneckRule, high_ttft_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(high_ttft_snapshot)
        assert any("3.000" in e for e in findings[0].evidence)

    def test_evidence_contains_tpot_when_present(
        self, rule: TTFTBottleneckRule, high_ttft_stable_tpot_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(high_ttft_stable_tpot_snapshot)
        assert any("TPOT" in e for e in findings[0].evidence)

    def test_evidence_contains_waiting_when_present(
        self,
        rule: TTFTBottleneckRule,
        high_ttft_stable_tpot_waiting_snapshot: MetricSnapshot,
    ) -> None:
        findings = rule.evaluate(high_ttft_stable_tpot_waiting_snapshot)
        assert any("10" in e for e in findings[0].evidence)

    def test_high_tpot_does_not_boost_confidence(
        self, rule: TTFTBottleneckRule
    ) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(ttft_p95_seconds=3.0, tpot_p95_seconds=0.5),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.low

    def test_no_finding_when_ttft_is_nan(self, rule: TTFTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(ttft_p95_seconds=float("nan"))
        )
        assert rule.evaluate(snapshot) == []

    def test_no_finding_when_tpot_is_nan(self, rule: TTFTBottleneckRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(ttft_p95_seconds=3.0, tpot_p95_seconds=float("nan")),
        )
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert not any("nan" in e for e in findings[0].evidence)

    def test_custom_threshold(self) -> None:
        rule = TTFTBottleneckRule(high_ttft_p95=5.0)
        assert (
            rule.evaluate(
                MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=4.9))
            )
            == []
        )
        assert (
            len(
                rule.evaluate(
                    MetricSnapshot(window="now", metrics=Metrics(ttft_p95_seconds=5.0))
                )
            )
            == 1
        )
