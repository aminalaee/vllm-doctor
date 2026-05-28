import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.preemption_pressure import PreemptionPressureRule


@pytest.fixture
def rule() -> PreemptionPressureRule:
    return PreemptionPressureRule()


class TestPreemptionPressureRule:
    def test_no_finding_when_metric_missing(self, rule: PreemptionPressureRule) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_when_zero_preemptions(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(num_preemptions_total=0))
        assert rule.evaluate(snapshot) == []

    def test_finding_when_preemptions_nonzero(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(num_preemptions_total=5))
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_medium_confidence_without_cache_signal(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(num_preemptions_total=10))
        assert rule.evaluate(snapshot)[0].confidence == Confidence.medium

    def test_high_confidence_with_high_cache(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(num_preemptions_total=10, kv_cache_usage_perc=0.85),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.high

    def test_medium_confidence_with_low_cache(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(num_preemptions_total=10, kv_cache_usage_perc=0.5),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.medium

    def test_evidence_contains_preemption_count(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(num_preemptions_total=42))
        assert any("42" in e for e in rule.evaluate(snapshot)[0].evidence)

    def test_evidence_contains_cache_usage_when_high(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(num_preemptions_total=5, kv_cache_usage_perc=0.90),
        )
        assert any("90%" in e for e in rule.evaluate(snapshot)[0].evidence)

    def test_summary_contains_preemption_count(self, rule: PreemptionPressureRule) -> None:
        snapshot = MetricSnapshot(window="now", metrics=Metrics(num_preemptions_total=7))
        assert "7" in rule.evaluate(snapshot)[0].summary

    async def test_preemption_pressure_with_scrape_fixture(self, rule: PreemptionPressureRule) -> None:
        snapshot = await snapshot_from_scrape_fixture("preemption-pressure.txt")
        assert len(rule.evaluate(snapshot)) == 1

    async def test_preemption_pressure_with_prometheus_fixture(self, rule: PreemptionPressureRule) -> None:
        snapshot = await snapshot_from_prometheus_fixture("preemption-pressure.json")
        assert len(rule.evaluate(snapshot)) == 1

    def test_custom_cache_threshold(self) -> None:
        rule = PreemptionPressureRule(high_cache_usage=0.95)
        snapshot = MetricSnapshot(
            window="now",
            metrics=Metrics(num_preemptions_total=5, kv_cache_usage_perc=0.90),
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.medium
