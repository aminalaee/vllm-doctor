import asyncio
from collections.abc import Callable
from pathlib import Path

import httpx
import typer
from rich.console import Console
from rich.live import Live

from vllm_doctor.cli import Format, app
from vllm_doctor.clients import Client, resolve_client
from vllm_doctor.clients.exceptions import ClientError
from vllm_doctor.clients.scrape import ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.config import Config, load_config
from vllm_doctor.diagnosis import run
from vllm_doctor.models import ClientMode, DiagnosisContext, DiagnosisResult
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports import text as text_report
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.utils.registry import build_rules
from vllm_doctor.stores import HistoryStore
from vllm_doctor.stores.migrate import run_migrations

_WATCH_INTERVAL_SECONDS = 5


async def _diagnose(
    client: Client,
    rules: list[Rule],
    since: str,
    model: str | None = None,
) -> DiagnosisResult:
    client_mode = ClientMode.scrape if isinstance(client, ScrapeClient) else ClientMode.prometheus
    context = DiagnosisContext(since=since, model_name=model, client_mode=client_mode)
    collection = await collect(client, since=since, model=model)
    checks = run(metrics=collection.series, rules=rules)
    return DiagnosisResult(
        context=context,
        metric_series=collection.series,
        checks=checks,
    )


def _firing_ids(result: DiagnosisResult) -> set[str]:
    return {check.id for check in result.checks if check.finding is not None}


def _transition_label(prev: DiagnosisResult | None, curr: DiagnosisResult) -> str:
    if prev is None:
        return "initial"
    if prev.health != curr.health:
        return f"{prev.health.value} → {curr.health.value}"
    if _firing_ids(prev) != _firing_ids(curr):
        return "rules changed"
    return ""


async def _watch_loop(
    client: Client,
    rules: list[Rule],
    since: str,
    render: Callable[[DiagnosisResult], None],
    model: str | None = None,
    *,
    save: bool = False,
    db_url: str | None = None,
) -> None:
    store: HistoryStore | None = None
    if save and db_url:
        run_migrations(db_url)
        store = HistoryStore(db_url)
    previous: DiagnosisResult | None = None
    try:
        while True:
            result = await _diagnose(client, rules, since, model)
            render(result)
            if save and store is not None:
                label = _transition_label(previous, result)
                if label:
                    run_id = store.save(result)
                    typer.echo(f"Saved run: {run_id} ({label})", err=True)
            previous = result
            await asyncio.sleep(_WATCH_INTERVAL_SECONDS)
    finally:
        if store is not None:
            store.close()


async def _run(
    url: str,
    since: str,
    output: Format,
    verbose: bool,
    watch: bool,
    save: bool,
    config: Config,
    model: str | None = None,
) -> None:
    rules = build_rules(config.rules)
    console = Console()

    async with await resolve_client(url) as client:
        if not watch:
            result = await _diagnose(client, rules, since, model)
            if output == Format.json:
                typer.echo(json_report.render(result, verbose=verbose))
            else:
                text_report.render(result, verbose=verbose)
            if save:
                run_migrations(config.database.url)
                with HistoryStore(config.database.url) as store:
                    run_id = store.save(result)
                typer.echo(f"Saved run: {run_id}", err=True)
        elif output == Format.json:

            def render_json(r: DiagnosisResult) -> None:
                typer.echo(json_report.render(r, verbose=verbose, compact=True))

            await _watch_loop(client, rules, since, render_json, model, save=save, db_url=config.database.url)
        else:
            with Live("", console=console, auto_refresh=False) as live_display:

                def render_text(r: DiagnosisResult) -> None:
                    live_display.update(text_report.build(r, verbose=verbose))
                    live_display.refresh()

                await _watch_loop(client, rules, since, render_text, model, save=save, db_url=config.database.url)


@app.command()
def diagnose(
    url: str = typer.Argument(
        ...,
        metavar="URL",
        help="vLLM /metrics or Prometheus URL to diagnose (e.g. http://host:8000/metrics or http://host:9090).",
    ),
    since: str = typer.Option("now", "--since", "-s", help="Time window (e.g. '1h', '30m', 'now')."),
    model: str | None = typer.Option(
        None, "--model", "-m", help="Filter metrics by model_name label (for a target serving several models)."
    ),
    watch: bool = typer.Option(
        False,
        "--watch",
        "-w",
        help="Refresh continuously every 5s (pipe through `watch -n N` for a different interval).",
    ),
    output: Format = typer.Option(Format.text, "--output", "-o", help="Output format (text or json)."),
    save: bool = typer.Option(False, "--save", help="Persist this diagnosis run to the local database."),
    verbose: bool = typer.Option(False, "--verbose", "-v", help="Show additional diagnostic detail."),
    config_path: Path | None = typer.Option(
        None, "--config", "-c", help="Path to config file (default: vllm-doctor.toml)."
    ),
) -> None:
    try:
        config = load_config(config_path)
        asyncio.run(_run(url, since, output, verbose, watch, save, config, model))
    except KeyboardInterrupt:
        pass
    except (ClientError, httpx.HTTPError) as e:
        typer.secho(f"Error: could not read metrics from {url}: {e}", fg=typer.colors.RED, err=True)
        raise typer.Exit(code=1) from e
