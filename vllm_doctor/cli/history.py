import json as json_mod
from datetime import datetime
from pathlib import Path

import typer
from rich.box import SIMPLE
from rich.console import Console
from rich.table import Table
from rich.text import Text

from vllm_doctor.cli import Format, app
from vllm_doctor.config import load_config
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports import text as text_report
from vllm_doctor.reports.text import HEALTH_COLOR
from vllm_doctor.stores import HistoryStore

history_app = typer.Typer(help="Local diagnosis history commands")
app.add_typer(history_app, name="history")


@history_app.command()
def list(
    output: Format = typer.Option(Format.text, "--output", "-o", help="Output format (text or json)."),
    verbose: bool = typer.Option(False, "--verbose", "-v", help="Show additional columns."),
    config_path: Path | None = typer.Option(
        None, "--config", "-c", help="Path to config file (default: vllm-doctor.toml)."
    ),
) -> None:
    config = load_config(config_path)
    with HistoryStore(config.database.url) as store:
        runs = store.list()

    if output == Format.json:
        data = [run.model_dump() for run in runs]
        typer.echo(json_mod.dumps(data))
        return

    if not runs:
        typer.echo("No saved diagnosis runs found.")
        return

    table = Table(show_header=True, box=SIMPLE, padding=(0, 1), header_style="bold")
    table.add_column("Run ID", no_wrap=True, min_width=20)
    table.add_column("Time", no_wrap=True, min_width=16)
    table.add_column("Model", no_wrap=True, min_width=20)
    if verbose:
        table.add_column("Mode", no_wrap=True, min_width=10)
    table.add_column("Health", no_wrap=True, min_width=8)
    table.add_column("Fired", justify="right", no_wrap=True, min_width=5)

    for run in runs:
        color = HEALTH_COLOR.get(run.health, "white")
        saved = datetime.fromisoformat(run.saved_at).strftime("%Y-%m-%d %H:%M")
        row = [
            run.run_id,
            saved,
            run.model_name or "—",
        ]
        if verbose:
            row.append(run.client_mode.value)
        row += [
            Text(run.health.value, style=f"bold {color}"),
            str(run.fired_count),
        ]
        table.add_row(*row)

    console = Console()
    console.print(table)


@history_app.command()
def show(
    run_id: str = typer.Argument(..., help="ID of the saved diagnosis run."),
    output: Format = typer.Option(Format.text, "--output", "-o", help="Output format (text or json)."),
    verbose: bool = typer.Option(False, "--verbose", "-v", help="Show additional diagnostic detail."),
    config_path: Path | None = typer.Option(
        None, "--config", "-c", help="Path to config file (default: vllm-doctor.toml)."
    ),
) -> None:
    config = load_config(config_path)
    with HistoryStore(config.database.url) as store:
        result = store.get(run_id)

    if result is None:
        typer.secho(f"Error: run {run_id} not found.", fg=typer.colors.RED, err=True)
        raise typer.Exit(code=1)

    if output == Format.json:
        typer.echo(json_report.render(result, verbose=verbose))
    else:
        text_report.render(result, verbose=verbose)
