import pytest

from tests.helpers import snapshot_from_fixture
from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.low_throughput import LowThroughputRule


@pytest.fixture
def rule() -> LowThroughputRule:
    return LowThroughputRule()


@pytest.fixture
def low_throughput_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(prompt_tokens_per_second=5.0, generation_tokens_per_second=20.0),
    )


@pytest.fixture
def low_throughput_with_low_running_snapshot() -> MetricSnapshot:
    return MetricSnapshot(
        window="now",
        metrics=Metrics(
            prompt_tokens_per_second=5.0,
            generation_tokens_per_second=20.0,
            num_requests_running=1,
        ),
    )


class TestLowThroughputRule:
    def test_no_finding_when_metrics_missing(self, rule: LowThroughputRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_above_threshold(self, rule: LowThroughputRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(prompt_tokens_per_second=20.0, generation_tokens_per_second=100.0),
        )
        assert rule.evaluate(snapshot) == []

    def test_no_finding_when_requests_waiting(self, rule: LowThroughputRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(
                prompt_tokens_per_second=5.0,
                generation_tokens_per_second=20.0,
                num_requests_waiting=3,
            ),
        )
        assert rule.evaluate(snapshot) == []

    def test_finding_when_both_low(self, rule: LowThroughputRule, low_throughput_snapshot: MetricSnapshot) -> None:
        findings = rule.evaluate(low_throughput_snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_medium_confidence_when_both_low(
        self, rule: LowThroughputRule, low_throughput_snapshot: MetricSnapshot
    ) -> None:
        assert rule.evaluate(low_throughput_snapshot)[0].confidence == Confidence.medium

    def test_medium_confidence_when_running_low(
        self,
        rule: LowThroughputRule,
        low_throughput_with_low_running_snapshot: MetricSnapshot,
    ) -> None:
        assert rule.evaluate(low_throughput_with_low_running_snapshot)[0].confidence == Confidence.medium

    def test_low_confidence_when_only_prompt_low(self, rule: LowThroughputRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(prompt_tokens_per_second=5.0, generation_tokens_per_second=100.0),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.low

    def test_finding_when_only_prompt_low(self, rule: LowThroughputRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(prompt_tokens_per_second=5.0, generation_tokens_per_second=100.0),
        )
        assert len(rule.evaluate(snapshot)) == 1

    def test_finding_when_only_gen_low(self, rule: LowThroughputRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(prompt_tokens_per_second=20.0, generation_tokens_per_second=20.0),
        )
        assert len(rule.evaluate(snapshot)) == 1

    def test_evidence_contains_prompt_tps(
        self, rule: LowThroughputRule, low_throughput_snapshot: MetricSnapshot
    ) -> None:
        findings = rule.evaluate(low_throughput_snapshot)
        assert any("5.0" in e for e in findings[0].evidence)

    def test_evidence_contains_gen_tps(self, rule: LowThroughputRule, low_throughput_snapshot: MetricSnapshot) -> None:
        findings = rule.evaluate(low_throughput_snapshot)
        assert any("20.0" in e for e in findings[0].evidence)

    async def test_low_throughput_with_fixture(self, rule: LowThroughputRule) -> None:
        snapshot = await snapshot_from_fixture("low-throughput.txt")
        assert len(rule.evaluate(snapshot)) == 1

    def test_custom_thresholds(self) -> None:
        rule = LowThroughputRule(low_prompt_tps=5.0, low_gen_tps=25.0)
        assert (
            rule.evaluate(
                MetricSnapshot(
                    window="now",
                    metrics=Metrics(prompt_tokens_per_second=6.0, generation_tokens_per_second=30.0),
                )
            )
            == []
        )
