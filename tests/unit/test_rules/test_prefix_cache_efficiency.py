import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, Severity
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule


@pytest.fixture
def rule() -> PrefixCacheEfficiencyRule:
    return PrefixCacheEfficiencyRule()


class TestPrefixCacheEfficiencyRule:
    def test_no_finding_when_metric_missing(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.run(MetricSeriesSnapshot()) is None

    def test_no_finding_above_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.5)) is None

    def test_no_finding_at_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.6)) is None

    def test_finding_below_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        result = rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.3))
        assert result is not None
        assert rule.severity == Severity.warning

    def test_zero_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        result = rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.0))
        assert result is not None

    def test_medium_confidence_moderate_low_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.3)).confidence == Confidence.medium

    def test_high_confidence_very_low_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.1)).confidence == Confidence.high

    def test_evidence_contains_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        result = rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.1))
        assert any("10%" in e for e in result.evidence)

    async def test_prefix_cache_efficiency_with_scrape_fixture(self, rule: PrefixCacheEfficiencyRule) -> None:
        metrics = await snapshot_from_scrape_fixture("prefix-cache.txt")
        assert rule.run(metrics) is not None

    async def test_prefix_cache_efficiency_with_prometheus_fixture(self, rule: PrefixCacheEfficiencyRule) -> None:
        metrics = await snapshot_from_prometheus_fixture("prefix-cache.json")
        assert rule.run(metrics) is not None

    def test_custom_threshold(self) -> None:
        rule = PrefixCacheEfficiencyRule(min_hit_rate=0.8)
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.8)) is None
        assert rule.run(MetricSeriesSnapshot(prefix_cache_hit_rate=0.79)) is not None
