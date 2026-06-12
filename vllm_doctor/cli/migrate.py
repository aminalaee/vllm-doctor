from pathlib import Path

import typer

from vllm_doctor.cli import app
from vllm_doctor.config import load_config
from vllm_doctor.stores.migrate import run_migrations


@app.command()
def migrate(
    config_path: Path | None = typer.Option(
        None, "--config", "-c", help="Path to config file (default: vllm-doctor.toml)."
    ),
) -> None:
    """Run database migrations."""
    config = load_config(config_path)
    run_migrations(config.database.url)
    typer.echo("Database migrated successfully.")
