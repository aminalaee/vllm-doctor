import asyncio
from enum import Enum
from pathlib import Path

import typer
from rich.console import Console
from rich.live import Live

from vllm_doctor.clients import Client, resolve_client
from vllm_doctor.collector import collect
from vllm_doctor.config import Config, load_config
from vllm_doctor.diagnosis import run
from vllm_doctor.models import DiagnosisResult
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports import text as text_report
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.error_rate import ErrorRateRule
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.preemption_pressure import PreemptionPressureRule
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule
from vllm_doctor.rules.queue_latency import QueueLatencyRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

app = typer.Typer(help="Diagnostic tool for vLLM inference servers")


def _build_rules(config: Config) -> list[Rule]:
    c = config.rules
    return [
        QueuePressureRule(
            high_waiting=c.queue_pressure.high_waiting,
            high_running=c.queue_pressure.high_running,
        ),
        QueueLatencyRule(high_queue_time_p95=c.queue_latency.high_queue_time_p95),
        KVCachePressureRule(high_cache_usage=c.kv_cache_pressure.high_cache_usage),
        PreemptionPressureRule(high_cache_usage=c.preemption_pressure.high_cache_usage),
        LowThroughputRule(
            low_prompt_tps=c.low_throughput.low_prompt_tps,
            low_gen_tps=c.low_throughput.low_gen_tps,
            low_running=c.low_throughput.low_running,
        ),
        ErrorRateRule(
            high_error_rate=c.error_rate.high_error_rate,
            high_abort_rate=c.error_rate.high_abort_rate,
        ),
        TTFTBottleneckRule(
            high_ttft_p95=c.ttft_bottleneck.high_ttft_p95,
            high_tpot_p95=c.ttft_bottleneck.high_tpot_p95,
        ),
        TPOTBottleneckRule(
            high_tpot_p95=c.tpot_bottleneck.high_tpot_p95,
            low_gen_tokens_per_sec=c.tpot_bottleneck.low_gen_tokens_per_sec,
        ),
        PrefixCacheEfficiencyRule(min_hit_rate=c.prefix_cache_efficiency.min_hit_rate),
    ]


class Format(str, Enum):
    text = "text"
    json = "json"


async def _diagnose(client: Client, rules: list[Rule], window: str) -> DiagnosisResult:
    snapshot = await collect(client, window=window)
    return DiagnosisResult(snapshot=snapshot, checks=run(snapshot, rules))


async def _run(url: str, window: str, fmt: Format, verbose: bool, live: int | None, config: Config) -> None:
    if live is not None and live <= 0:
        raise typer.BadParameter("must be a positive integer", param_hint="'--live'")

    rules = _build_rules(config)
    console = Console()
    async with await resolve_client(url) as client:
        if fmt == Format.json:
            while True:
                result = await _diagnose(client, rules, window)
                if live is not None:
                    console.clear()
                typer.echo(json_report.render(result, verbose=verbose))
                if live is None:
                    return
                await asyncio.sleep(live)
        else:
            with Live("", console=console, auto_refresh=False) as live_display:
                while True:
                    result = await _diagnose(client, rules, window)
                    live_display.update(text_report.build(result, verbose=verbose))
                    live_display.refresh()
                    if live is None:
                        return
                    await asyncio.sleep(live)


@app.command()
def main(
    url: str = typer.Option(
        ...,
        "--url",
        "-u",
        help="URL to diagnose (e.g. http://host:8000/metrics or http://host:9090).",
    ),
    window: str = typer.Option("now", "--window", "-w", help="Time window (e.g. '1h', '30m', 'now')."),
    fmt: Format = typer.Option(Format.text, "--format", "-f", help="Output format (text or json)."),
    verbose: bool = typer.Option(False, "--verbose", "-v", help="Show additional diagnostic detail."),
    live: int | None = typer.Option(None, "--live", "-l", help="Refresh interval in seconds (e.g. --live 10)."),
    config_path: Path | None = typer.Option(
        None, "--config", "-c", help="Path to config file (default: vllm-doctor.toml)."
    ),
) -> None:
    try:
        config = load_config(config_path)
        asyncio.run(_run(url, window, fmt, verbose, live, config))
    except KeyboardInterrupt:
        pass
