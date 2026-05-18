import math

from rich.console import Console
from rich.panel import Panel
from rich.rule import Rule
from rich.table import Table
from rich.text import Text

from vllm_doctor.models import DiagnosisResult, Finding, Health, Metrics, Severity

_SEVERITY_COLOR = {
    Severity.critical: "red",
    Severity.warning: "yellow",
    Severity.info: "blue",
}

_HEALTH_COLOR = {
    Health.ok: "green",
    Health.info: "blue",
    Health.warning: "yellow",
    Health.critical: "red",
}

_SEVERITY_ICON = {
    Severity.critical: "✖",
    Severity.warning: "⚠",
    Severity.info: "ℹ",
}

_BAR_WIDTH = 20
_BAR_FILLED = "█"
_BAR_EMPTY = "░"


def _cache_bar(value: float, color: str) -> Text:
    filled = round(value * _BAR_WIDTH)
    bar = _BAR_FILLED * filled + _BAR_EMPTY * (_BAR_WIDTH - filled)
    pct = f"{value:.0%}"
    return Text.assemble((bar, f"bold {color}"), (f" {pct}", f"bold {color}"))


def _finding_panel(finding: Finding, console: Console) -> None:
    color = _SEVERITY_COLOR[finding.severity]
    icon = _SEVERITY_ICON[finding.severity]

    title = Text.assemble(
        (f"{icon} ", f"bold {color}"),
        (finding.title, "bold"),
        (f"  [{finding.confidence.value} confidence]", "dim"),
    )

    body = Text()
    if finding.evidence:
        body.append("  " + "  ·  ".join(finding.evidence) + "\n", style="dim")
    if finding.recommendations:
        body.append("\n")
        for r in finding.recommendations:
            body.append("  → ", style=f"bold {color}")
            body.append(f"{r}\n")

    body.rstrip()
    console.print(Panel(body, title=title, title_align="left", border_style=color))


def _metrics_table(result: DiagnosisResult, console: Console) -> None:
    table = Table(show_header=False, box=None, padding=(0, 2))
    table.add_column(style="dim", no_wrap=True)
    table.add_column(justify="right", no_wrap=True)

    for name, field in Metrics.model_fields.items():
        value = getattr(result.snapshot.metrics, name)
        if value is None:
            continue
        label = field.title or name
        extra = field.json_schema_extra or {}
        fmt = str(extra.get("fmt", ".0f"))

        if extra.get("bar"):
            if not math.isfinite(value):
                table.add_row(label, Text("n/a", style="dim"))
            else:
                color = "red" if value >= 0.9 else "yellow" if value >= 0.7 else "green"
                table.add_row(label, _cache_bar(value, color))
        else:
            table.add_row(label, Text(format(value, fmt), style="bold"))

    console.print(table)


def render(
    result: DiagnosisResult, console: Console | None = None, verbose: bool = False
) -> None:
    console = console or Console()

    h = result.health
    color = _HEALTH_COLOR[h]
    health = Text.assemble(("Health: ", "bold"), (h.value.upper(), f"bold {color}"))

    console.print(
        Rule(
            Text.assemble(
                ("vLLM Doctor", "bold"),
                ("  ·  ", "dim"),
                health,
                ("  ·  ", "dim"),
                Text(f"Window: {result.snapshot.window}", style="dim"),
            ),
            style="dim",
        )
    )
    console.print()

    if not result.findings:
        console.print(Text("  No issues detected.", style="green"))
        console.print()
    else:
        for finding in result.findings:
            _finding_panel(finding, console)
        console.print()

    if verbose:
        console.print(Rule("Observed Metrics", style="dim"))
        console.print()
        _metrics_table(result, console)
        console.print()
