import asyncio
from collections.abc import Callable
from enum import Enum
from pathlib import Path

import typer
from rich.console import Console
from rich.live import Live

from vllm_doctor.clients import Client, resolve_client
from vllm_doctor.clients.scrape import ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.config import Config, load_config
from vllm_doctor.diagnosis import run
from vllm_doctor.models import ClientMode, DiagnosisContext, DiagnosisResult
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports import text as text_report
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.utils.registry import build_rules

app = typer.Typer(help="Diagnostic tool for vLLM inference servers")


class Format(str, Enum):
    text = "text"
    json = "json"


async def _diagnose(
    client: Client,
    rules: list[Rule],
    window: str,
    model: str | None = None,
) -> DiagnosisResult:
    client_mode = ClientMode.scrape if isinstance(client, ScrapeClient) else ClientMode.prometheus
    context = DiagnosisContext(window=window, model_name=model, client_mode=client_mode)
    metrics = await collect(client, window=window, model=model)
    checks = run(metrics=metrics, rules=rules)
    return DiagnosisResult(context=context, metrics=metrics, checks=checks)


async def _live_loop(
    client: Client,
    rules: list[Rule],
    window: str,
    interval: int,
    render: Callable[[DiagnosisResult], None],
) -> None:
    while True:
        result = await _diagnose(client, rules, window)
        render(result)
        await asyncio.sleep(interval)


async def _run(url: str, window: str, fmt: Format, verbose: bool, live: int | None, config: Config) -> None:
    if live is not None and live <= 0:
        raise typer.BadParameter("must be a positive integer", param_hint="'--live'")

    rules = build_rules(config.rules)
    console = Console()

    async with await resolve_client(url) as client:
        if live is None:
            result = await _diagnose(client, rules, window)
            if fmt == Format.json:
                typer.echo(json_report.render(result, verbose=verbose))
            else:
                console.print(text_report.build(result, verbose=verbose))
        elif fmt == Format.json:

            def render_json(r: DiagnosisResult) -> None:
                console.clear()
                typer.echo(json_report.render(r, verbose=verbose))

            await _live_loop(client, rules, window, live, render_json)
        else:
            with Live("", console=console, auto_refresh=False) as live_display:

                def render_text(r: DiagnosisResult) -> None:
                    live_display.update(text_report.build(r, verbose=verbose))
                    live_display.refresh()

                await _live_loop(client, rules, window, live, render_text)


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
