import typer

from vllm_doctor import __version__

app = typer.Typer(help="Diagnostic tool for vLLM inference servers")


def _version_callback(value: bool) -> None:
    if value:
        typer.echo(f"vllm-doctor {__version__}")
        raise typer.Exit()


@app.callback()
def _root(
    version: bool = typer.Option(
        False,
        "--version",
        help="Show version and exit.",
        callback=_version_callback,
        is_eager=True,
    ),
) -> None:
    pass


from . import diagnose, history  # noqa: E402, F401
