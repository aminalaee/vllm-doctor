import httpx
import pytest

from vllm_doctor.clients import ScrapeClient, resolve_client
from vllm_doctor.collector import collect
from vllm_doctor.diagnosis import run
from vllm_doctor.models import DiagnosisContext, DiagnosisResult, Metrics
from vllm_doctor.rules.error_rate import ErrorRateRule
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.preemption_pressure import PreemptionPressureRule
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule
from vllm_doctor.rules.queue_latency import QueueLatencyRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

METRICS_URL = "http://localhost:8000/metrics"


@pytest.fixture
async def client() -> ScrapeClient:
    c = await resolve_client(METRICS_URL)
    assert isinstance(c, ScrapeClient), "Expected ScrapeClient for /metrics endpoint"
    return c


class TestLiveScrape:
    async def test_resolve_returns_scrape_client(self) -> None:
        c = await resolve_client(METRICS_URL)
        assert isinstance(c, ScrapeClient)
        await c.aclose()

    async def test_requests_running_is_numeric(self, client: ScrapeClient) -> None:
        samples = await client.query("vllm:num_requests_running")
        assert len(samples) >= 1
        assert isinstance(samples[0].value, float)

    async def test_requests_waiting_is_numeric(self, client: ScrapeClient) -> None:
        samples = await client.query("vllm:num_requests_waiting")
        assert len(samples) >= 1
        assert isinstance(samples[0].value, float)

    async def test_snapshot_fields_populated(self, client: ScrapeClient) -> None:
        current = await collect(client, window="now")
        assert isinstance(current, Metrics)
        assert current.num_requests_running is not None
        assert current.num_requests_waiting is not None
        assert current.num_requests_running >= 0
        assert current.num_requests_waiting >= 0

    async def test_diagnosis_runs_without_error(self, client: ScrapeClient) -> None:
        ctx = DiagnosisContext(window="now")
        current = await collect(client, window="now")
        findings = run(context=ctx, current=current, rules=[QueuePressureRule()])
        assert isinstance(findings, list)

    async def test_ttft_percentile_returns_none_in_scrape_mode(self, client: ScrapeClient) -> None:
        result = await client.query_percentile("vllm:time_to_first_token_seconds", 0.95)
        assert result is None

    async def test_prefix_cache_hit_rate_populated(self, client: ScrapeClient) -> None:
        current = await collect(client, window="now")
        # hit rate is None only when no queries have been made yet
        assert current.prefix_cache_hit_rate is None or (0.0 <= current.prefix_cache_hit_rate <= 1.0)

    async def test_queue_time_p95_none_in_scrape_mode(self, client: ScrapeClient) -> None:
        current = await collect(client, window="now")
        assert current.queue_time_p95_seconds is None

    async def test_preemptions_is_numeric(self, client: ScrapeClient) -> None:
        current = await collect(client, window="now")
        assert current.num_preemptions_total is not None
        assert current.num_preemptions_total >= 0

    async def test_diagnosis_runs_with_all_rules(self, client: ScrapeClient) -> None:
        ctx = DiagnosisContext(window="now")
        current = await collect(client, window="now")
        all_rules = [
            QueuePressureRule(),
            QueueLatencyRule(),
            KVCachePressureRule(),
            PreemptionPressureRule(),
            LowThroughputRule(),
            ErrorRateRule(),
            TTFTBottleneckRule(),
            TPOTBottleneckRule(),
            PrefixCacheEfficiencyRule(),
        ]
        result = DiagnosisResult(
            context=ctx, current=current, checks=run(context=ctx, current=current, rules=all_rules)
        )
        assert isinstance(result.checks, list)

    async def test_raw_metrics_contain_vllm_prefix(self) -> None:
        r = httpx.get(METRICS_URL, timeout=5.0)
        assert "vllm:" in r.text
