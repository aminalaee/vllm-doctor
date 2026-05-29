"""Tests for trend detection helper and trend signals in rules."""

from vllm_doctor.models import DiagnosisContext, Metrics
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule
from vllm_doctor.rules.queue_latency import QueueLatencyRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule
from vllm_doctor.rules.trend import falling, rising
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

_CTX = DiagnosisContext(window="1m")


class TestRising:
    def test_rising_above_threshold(self) -> None:
        assert rising(1.2, 1.0) is True

    def test_not_rising_below_threshold(self) -> None:
        assert rising(1.05, 1.0) is False

    def test_not_rising_when_falling(self) -> None:
        assert rising(0.8, 1.0) is False

    def test_false_when_current_none(self) -> None:
        assert rising(None, 1.0) is False

    def test_false_when_previous_none(self) -> None:
        assert rising(1.2, None) is False

    def test_false_when_previous_zero(self) -> None:
        assert rising(1.2, 0.0) is False

    def test_custom_threshold(self) -> None:
        assert rising(1.04, 1.0, threshold=0.05) is False
        assert rising(1.06, 1.0, threshold=0.05) is True

    def test_just_below_threshold(self) -> None:
        assert rising(1.099, 1.0) is False

    def test_just_above_threshold(self) -> None:
        assert rising(1.101, 1.0) is True


class TestFalling:
    def test_falling_above_threshold(self) -> None:
        assert falling(0.8, 1.0) is True

    def test_not_falling_below_threshold(self) -> None:
        assert falling(0.95, 1.0) is False

    def test_not_falling_when_rising(self) -> None:
        assert falling(1.2, 1.0) is False

    def test_false_when_current_none(self) -> None:
        assert falling(None, 1.0) is False

    def test_false_when_previous_none(self) -> None:
        assert falling(0.8, None) is False


class TestTTFTTrend:
    def test_rising_signal_added(self) -> None:
        curr, prev = Metrics(ttft_p95_seconds=3.0), Metrics(ttft_p95_seconds=2.0)
        assert any("rising" in s for s in TTFTBottleneckRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        findings = TTFTBottleneckRule().run(_CTX, Metrics(ttft_p95_seconds=3.0))
        assert not any("rising" in s for s in findings[0].signals)

    def test_no_trend_signal_when_stable(self) -> None:
        curr, prev = Metrics(ttft_p95_seconds=3.0), Metrics(ttft_p95_seconds=2.9)
        assert not any("rising" in s for s in TTFTBottleneckRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_trend_signal_contains_values(self) -> None:
        curr, prev = Metrics(ttft_p95_seconds=3.0), Metrics(ttft_p95_seconds=2.0)
        signal = next(s for s in TTFTBottleneckRule().run(_CTX, curr, previous=prev)[0].signals if "rising" in s)
        assert "2.00s" in signal and "3.00s" in signal


class TestTPOTTrend:
    def test_rising_signal_added(self) -> None:
        curr, prev = Metrics(tpot_p95_seconds=0.35), Metrics(tpot_p95_seconds=0.2)
        assert any("rising" in s for s in TPOTBottleneckRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        assert not any("rising" in s for s in TPOTBottleneckRule().run(_CTX, Metrics(tpot_p95_seconds=0.35))[0].signals)

    def test_trend_signal_contains_values(self) -> None:
        curr, prev = Metrics(tpot_p95_seconds=0.350), Metrics(tpot_p95_seconds=0.200)
        signal = next(s for s in TPOTBottleneckRule().run(_CTX, curr, previous=prev)[0].signals if "rising" in s)
        assert "0.200s" in signal and "0.350s" in signal


class TestKVCacheTrend:
    def test_rising_signal_added(self) -> None:
        curr, prev = Metrics(kv_cache_usage_perc=0.95), Metrics(kv_cache_usage_perc=0.80)
        assert any("rising" in s for s in KVCachePressureRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        findings = KVCachePressureRule().run(_CTX, Metrics(kv_cache_usage_perc=0.95))
        assert not any("rising" in s for s in findings[0].signals)

    def test_trend_signal_contains_percentages(self) -> None:
        curr, prev = Metrics(kv_cache_usage_perc=0.95), Metrics(kv_cache_usage_perc=0.80)
        signal = next(s for s in KVCachePressureRule().run(_CTX, curr, previous=prev)[0].signals if "rising" in s)
        assert "80%" in signal and "95%" in signal


class TestQueuePressureTrend:
    def test_growing_signal_added(self) -> None:
        curr, prev = Metrics(num_requests_waiting=15), Metrics(num_requests_waiting=5)
        assert any("growing" in s for s in QueuePressureRule(high_waiting=3).run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        findings = QueuePressureRule(high_waiting=3).run(_CTX, Metrics(num_requests_waiting=15))
        assert not any("growing" in s for s in findings[0].signals)

    def test_trend_signal_contains_values(self) -> None:
        curr, prev = Metrics(num_requests_waiting=15), Metrics(num_requests_waiting=5)
        signal = next(
            s for s in QueuePressureRule(high_waiting=3).run(_CTX, curr, previous=prev)[0].signals if "growing" in s
        )
        assert "5" in signal and "15" in signal


class TestQueueLatencyTrend:
    def test_worsening_signal_added(self) -> None:
        curr, prev = Metrics(queue_time_p95_seconds=1.5), Metrics(queue_time_p95_seconds=1.0)
        assert any("worsening" in s for s in QueueLatencyRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        findings = QueueLatencyRule().run(_CTX, Metrics(queue_time_p95_seconds=1.5))
        assert not any("worsening" in s for s in findings[0].signals)

    def test_trend_signal_contains_values(self) -> None:
        curr, prev = Metrics(queue_time_p95_seconds=1.50), Metrics(queue_time_p95_seconds=1.00)
        signal = next(s for s in QueueLatencyRule().run(_CTX, curr, previous=prev)[0].signals if "worsening" in s)
        assert "1.00s" in signal and "1.50s" in signal


class TestLowThroughputTrend:
    def test_declining_gen_signal_added(self) -> None:
        curr, prev = Metrics(generation_tokens_per_second=30.0), Metrics(generation_tokens_per_second=80.0)
        assert any("declining" in s for s in LowThroughputRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_declining_prompt_signal_when_gen_not_low(self) -> None:
        curr = Metrics(prompt_tokens_per_second=5.0, generation_tokens_per_second=60.0)
        prev = Metrics(prompt_tokens_per_second=20.0, generation_tokens_per_second=60.0)
        assert any("declining" in s for s in LowThroughputRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        findings = LowThroughputRule().run(_CTX, Metrics(generation_tokens_per_second=30.0))
        assert not any("declining" in s for s in findings[0].signals)

    def test_trend_signal_contains_values(self) -> None:
        curr, prev = Metrics(generation_tokens_per_second=30.0), Metrics(generation_tokens_per_second=80.0)
        signal = next(s for s in LowThroughputRule().run(_CTX, curr, previous=prev)[0].signals if "declining" in s)
        assert "80.0" in signal and "30.0" in signal


class TestPrefixCacheTrend:
    def test_declining_signal_added(self) -> None:
        curr, prev = Metrics(prefix_cache_hit_rate=0.25), Metrics(prefix_cache_hit_rate=0.45)
        assert any("declining" in s for s in PrefixCacheEfficiencyRule().run(_CTX, curr, previous=prev)[0].signals)

    def test_no_trend_signal_without_previous(self) -> None:
        assert PrefixCacheEfficiencyRule().run(_CTX, Metrics(prefix_cache_hit_rate=0.25))[0].signals == []

    def test_trend_signal_contains_percentages(self) -> None:
        curr, prev = Metrics(prefix_cache_hit_rate=0.25), Metrics(prefix_cache_hit_rate=0.45)
        signal = next(
            s for s in PrefixCacheEfficiencyRule().run(_CTX, curr, previous=prev)[0].signals if "declining" in s
        )
        assert "45%" in signal and "25%" in signal
