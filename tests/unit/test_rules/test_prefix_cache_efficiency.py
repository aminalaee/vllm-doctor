import pytest

from tests.helpers import snapshot_from_prometheus_fixture, snapshot_from_scrape_fixture
from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, Severity
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule

_CTX = DiagnosisContext(window="now")


@pytest.fixture
def rule() -> PrefixCacheEfficiencyRule:
    return PrefixCacheEfficiencyRule()


class TestPrefixCacheEfficiencyRule:
    def test_no_finding_when_metric_missing(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.evaluate(_CTX, Metrics()) == []

    def test_no_finding_above_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.5)) == []

    def test_no_finding_at_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.6)) == []

    def test_finding_below_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        findings = rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.3))
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_zero_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        findings = rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.0))
        assert len(findings) == 1

    def test_medium_confidence_moderate_low_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.3))[0].confidence == Confidence.medium

    def test_high_confidence_very_low_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        assert rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.1))[0].confidence == Confidence.high

    def test_evidence_contains_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        findings = rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.1))
        assert any("10%" in e for e in findings[0].evidence)

    async def test_prefix_cache_efficiency_with_scrape_fixture(self, rule: PrefixCacheEfficiencyRule) -> None:
        current = await snapshot_from_scrape_fixture("prefix-cache.txt")
        assert len(rule.evaluate(_CTX, current)) == 1

    async def test_prefix_cache_efficiency_with_prometheus_fixture(self, rule: PrefixCacheEfficiencyRule) -> None:
        current = await snapshot_from_prometheus_fixture("prefix-cache.json")
        assert len(rule.evaluate(_CTX, current)) == 1

    def test_custom_threshold(self) -> None:
        rule = PrefixCacheEfficiencyRule(min_hit_rate=0.8)
        assert rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.8)) == []
        assert len(rule.evaluate(_CTX, Metrics(prefix_cache_hit_rate=0.79))) == 1
