import asyncio

import typer

from vllm_doctor.clients import resolve_client
from vllm_doctor.collector import collect
from vllm_doctor.diagnosis import run
from vllm_doctor.report import render_text
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule

app = typer.Typer(help="Diagnostic tool for vLLM inference servers.")

_RULES = [QueuePressureRule(), KVCachePressureRule()]


async def _diagnose(url: str, window: str) -> None:
    async with await resolve_client(url) as client:
        snapshot = await collect(client, window=window)
        findings = run(snapshot, _RULES)
        render_text(findings, snapshot)


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
) -> None:
    try:
        asyncio.run(_diagnose(url, window))
    except KeyboardInterrupt:
        pass
