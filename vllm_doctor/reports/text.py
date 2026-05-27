import math

from rich.console import Console, Group
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


def _finding_panel(finding: Finding) -> Panel:
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
    return Panel(body, title=title, title_align="left", border_style=color)


def _matrix_table(result: DiagnosisResult) -> Table:
    table = Table(show_header=False, box=None, padding=(0, 2))
    table.add_column(style="dim", no_wrap=True)
    table.add_column(no_wrap=True)
    table.add_column(style="dim", no_wrap=True)

    for check in result.checks:
        if check.finding is None:
            table.add_row(check.name, Text("✓ ok", style="green"), "")
        else:
            f = check.finding
            color = _SEVERITY_COLOR[f.severity]
            icon = _SEVERITY_ICON[f.severity]
            status = Text(f"{icon} {f.severity.value}", style=f"bold {color}")
            confidence = Text(f"[{f.confidence.value}]", style="dim")
            table.add_row(check.name, status, confidence)

    return table


def _metrics_table(result: DiagnosisResult) -> Table:
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

    return table


def build(result: DiagnosisResult, verbose: bool = False) -> Group:
    h = result.health
    color = _HEALTH_COLOR[h]
    health = Text.assemble(("Health: ", "bold"), (h.value.upper(), f"bold {color}"))

    items: list = [
        Rule(
            Text.assemble(
                ("vLLM Doctor", "bold"),
                ("  ·  ", "dim"),
                health,
                ("  ·  ", "dim"),
                Text(f"Window: {result.snapshot.window}", style="dim"),
            ),
            style="dim",
        ),
        Text(),
    ]

    fired = [c.finding for c in result.checks if c.finding is not None]
    for finding in fired:
        items.append(_finding_panel(finding))
    if fired:
        items.append(Text())

    if result.checks:
        items += [_matrix_table(result), Text()]

    if verbose:
        items += [
            Rule("Observed Metrics", style="dim"),
            Text(),
            _metrics_table(result),
            Text(),
        ]

    return Group(*items)


def render(
    result: DiagnosisResult, console: Console | None = None, verbose: bool = False
) -> None:
    (console or Console()).print(build(result, verbose))
