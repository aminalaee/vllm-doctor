import asyncio
from enum import Enum

import typer

from vllm_doctor.clients import resolve_client
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


async def _diagnose(url: str, window: str, fmt: Format) -> None:
    async with await resolve_client(url) as client:
        snapshot = await collect(client, window=window)
        result = DiagnosisResult(snapshot=snapshot, findings=run(snapshot, _RULES))
        if fmt == Format.json:
            typer.echo(json_report.render(result))
        else:
            text_report.render(result)


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
) -> None:
    try:
        asyncio.run(_diagnose(url, window, fmt))
    except KeyboardInterrupt:
        pass
