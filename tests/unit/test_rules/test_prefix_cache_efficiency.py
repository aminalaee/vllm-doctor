import pytest

from vllm_doctor.models import Confidence, Metrics, MetricSnapshot, Severity
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule


@pytest.fixture
def rule() -> PrefixCacheEfficiencyRule:
    return PrefixCacheEfficiencyRule()


class TestPrefixCacheEfficiencyRule:
    def test_no_finding_when_metric_missing(
        self, rule: PrefixCacheEfficiencyRule
    ) -> None:
        assert rule.evaluate(MetricSnapshot(window="now")) == []

    def test_no_finding_above_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.5)
        )
        assert rule.evaluate(snapshot) == []

    def test_no_finding_at_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.6)
        )
        assert rule.evaluate(snapshot) == []

    def test_finding_below_threshold(self, rule: PrefixCacheEfficiencyRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.3)
        )
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1
        assert findings[0].severity == Severity.warning

    def test_zero_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.0)
        )
        findings = rule.evaluate(snapshot)
        assert len(findings) == 1

    def test_medium_confidence_moderate_low_rate(
        self, rule: PrefixCacheEfficiencyRule
    ) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.3)
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.medium

    def test_high_confidence_very_low_rate(
        self, rule: PrefixCacheEfficiencyRule
    ) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.1)
        )
        assert rule.evaluate(snapshot)[0].confidence == Confidence.high

    def test_evidence_contains_hit_rate(self, rule: PrefixCacheEfficiencyRule) -> None:
        snapshot = MetricSnapshot(
            window="now", metrics=Metrics(prefix_cache_hit_rate=0.1)
        )
        findings = rule.evaluate(snapshot)
        assert any("10%" in e for e in findings[0].evidence)

    def test_custom_threshold(self) -> None:
        rule = PrefixCacheEfficiencyRule(min_hit_rate=0.8)
        assert (
            rule.evaluate(
                MetricSnapshot(window="now", metrics=Metrics(prefix_cache_hit_rate=0.8))
            )
            == []
        )
        assert (
            len(
                rule.evaluate(
                    MetricSnapshot(
                        window="now", metrics=Metrics(prefix_cache_hit_rate=0.79)
                    )
                )
            )
            == 1
        )
