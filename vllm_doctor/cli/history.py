import typer

from vllm_doctor.cli import app

history_app = typer.Typer(help="Local diagnosis history commands")
app.add_typer(history_app, name="history")
