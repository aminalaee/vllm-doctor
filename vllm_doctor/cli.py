import asyncio
from enum import Enum

import typer
from rich.console import Console
from rich.live import Live

from vllm_doctor.clients import Client, resolve_client
from vllm_doctor.collector import collect
from vllm_doctor.diagnosis import run
from vllm_doctor.models import DiagnosisResult
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports import text as text_report
from vllm_doctor.rules.error_rate import ErrorRateRule
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

app = typer.Typer(help="Diagnostic tool for vLLM inference servers")

_RULES = [
    QueuePressureRule(),
    KVCachePressureRule(),
    LowThroughputRule(),
    ErrorRateRule(),
    TTFTBottleneckRule(),
    TPOTBottleneckRule(),
]


class Format(str, Enum):
    text = "text"
    json = "json"


async def _diagnose(client: Client, window: str) -> DiagnosisResult:
    snapshot = await collect(client, window=window)
    return DiagnosisResult(snapshot=snapshot, findings=run(snapshot, _RULES))


async def _run(
    url: str, window: str, fmt: Format, verbose: bool, live: int | None
) -> None:
    if live is not None and live <= 0:
        raise typer.BadParameter("must be a positive integer", param_hint="'--live'")

    console = Console()
    async with await resolve_client(url) as client:
        if fmt == Format.json:
            while True:
                result = await _diagnose(client, window)
                if live is not None:
                    console.clear()
                typer.echo(json_report.render(result, verbose=verbose))
                if live is None:
                    return
                await asyncio.sleep(live)
        else:
            with Live("", console=console, auto_refresh=False) as live_display:
                while True:
                    result = await _diagnose(client, window)
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
    window: str = typer.Option(
        "now", "--window", "-w", help="Time window (e.g. '1h', '30m', 'now')."
    ),
    fmt: Format = typer.Option(
        Format.text, "--format", "-f", help="Output format (text or json)."
    ),
    verbose: bool = typer.Option(
        False, "--verbose", "-v", help="Show additional diagnostic detail."
    ),
    live: int | None = typer.Option(
        None, "--live", "-l", help="Refresh interval in seconds (e.g. --live 10)."
    ),
) -> None:
    try:
        asyncio.run(_run(url, window, fmt, verbose, live))
    except KeyboardInterrupt:
        pass
