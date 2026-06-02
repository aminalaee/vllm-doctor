import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, Severity
from vllm_doctor.rules.low_throughput import LowThroughputRule


@pytest.fixture
def rule() -> LowThroughputRule:
    return LowThroughputRule()


@pytest.fixture
def low_throughput() -> MetricSeriesSnapshot:
    return MetricSeriesSnapshot(prompt_tokens_per_second=5.0, generation_tokens_per_second=20.0)


@pytest.fixture
def low_throughput_with_low_running() -> MetricSeriesSnapshot:
    return MetricSeriesSnapshot(prompt_tokens_per_second=5.0, generation_tokens_per_second=20.0, num_requests_running=1)


class TestLowThroughputRule:
    def test_no_finding_when_metrics_missing(self, rule: LowThroughputRule) -> None:
        assert rule.run(MetricSeriesSnapshot()) is None

    def test_no_finding_above_threshold(self, rule: LowThroughputRule) -> None:
        metrics = MetricSeriesSnapshot(prompt_tokens_per_second=20.0, generation_tokens_per_second=100.0)
        assert rule.run(metrics) is None

    def test_no_finding_when_requests_waiting(self, rule: LowThroughputRule) -> None:
        metrics = MetricSeriesSnapshot(
            prompt_tokens_per_second=5.0, generation_tokens_per_second=20.0, num_requests_waiting=3
        )
        assert rule.run(metrics) is None

    def test_finding_when_both_low(self, rule: LowThroughputRule, low_throughput: MetricSeriesSnapshot) -> None:
        result = rule.run(low_throughput)
        assert result is not None
        assert rule.severity == Severity.warning

    def test_medium_confidence_when_both_low(
        self, rule: LowThroughputRule, low_throughput: MetricSeriesSnapshot
    ) -> None:
        assert rule.run(low_throughput).confidence == Confidence.medium

    def test_medium_confidence_when_running_low(
        self, rule: LowThroughputRule, low_throughput_with_low_running: MetricSeriesSnapshot
    ) -> None:
        assert rule.run(low_throughput_with_low_running).confidence == Confidence.medium

    def test_low_confidence_when_only_prompt_low(self, rule: LowThroughputRule) -> None:
        metrics = MetricSeriesSnapshot(prompt_tokens_per_second=5.0, generation_tokens_per_second=100.0)
        assert rule.run(metrics).confidence == Confidence.low

    def test_finding_when_only_prompt_low(self, rule: LowThroughputRule) -> None:
        metrics = MetricSeriesSnapshot(prompt_tokens_per_second=5.0, generation_tokens_per_second=100.0)
        assert rule.run(metrics) is not None

    def test_finding_when_only_gen_low(self, rule: LowThroughputRule) -> None:
        metrics = MetricSeriesSnapshot(prompt_tokens_per_second=20.0, generation_tokens_per_second=20.0)
        assert rule.run(metrics) is not None

    def test_evidence_contains_prompt_tps(self, rule: LowThroughputRule, low_throughput: MetricSeriesSnapshot) -> None:
        result = rule.run(low_throughput)
        assert any("5.0" in e for e in result.evidence)

    def test_evidence_contains_gen_tps(self, rule: LowThroughputRule, low_throughput: MetricSeriesSnapshot) -> None:
        result = rule.run(low_throughput)
        assert any("20.0" in e for e in result.evidence)

    async def test_low_throughput_with_scrape_fixture(self, rule: LowThroughputRule) -> None:
        metrics = await snapshot_from_scrape_fixture("low-throughput.txt")
        assert rule.run(metrics) is not None

    async def test_low_throughput_with_prometheus_fixture(self, rule: LowThroughputRule) -> None:
        metrics = await snapshot_from_prometheus_fixture("low-throughput.json")
        assert rule.run(metrics) is not None

    def test_custom_thresholds(self) -> None:
        rule = LowThroughputRule(low_prompt_tps=5.0, low_gen_tps=25.0)
        metrics = MetricSeriesSnapshot(prompt_tokens_per_second=6.0, generation_tokens_per_second=30.0)
        assert rule.run(metrics) is None
