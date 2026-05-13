from rich.console import Console
from rich.console import Group, RenderableType
from rich.panel import Panel
from rich.progress_bar import ProgressBar
from rich.table import Table
from rich.text import Text

from vllm_doctor.models import Finding, MetricSnapshot, Severity

_SEVERITY_COLOR = {
    Severity.critical: "red",
    Severity.warning: "yellow",
    Severity.info: "blue",
}

_SEVERITY_LABEL = {
    Severity.critical: "CRITICAL",
    Severity.warning: "WARNING",
    Severity.info: "INFO",
}

_BAR_WIDTH = 20


def _bar(value: float, cap: float) -> ProgressBar:
    pct = min(value / cap, 1.0) if cap > 0 else 0.0
    color = "red" if pct >= 0.9 else "yellow" if pct >= 0.6 else "green"
    return ProgressBar(
        total=100, completed=pct * 100, width=_BAR_WIDTH, complete_style=color
    )


def _metrics_panel(snapshot: MetricSnapshot) -> Panel:
    table = Table(box=None, padding=(0, 2), show_header=False, show_edge=False)
    table.add_column("Metric", style="bold", min_width=22)
    table.add_column("Value", justify="right", min_width=6)
    table.add_column("", min_width=_BAR_WIDTH + 2)
    table.add_column("%", justify="right", min_width=5, style="dim")

    def _row(label: str, value: float | None, cap: float) -> None:
        if value is None:
            table.add_row(
                label, "n/a", ProgressBar(total=100, completed=0, width=_BAR_WIDTH), ""
            )
        else:
            pct = min(value / cap, 1.0) if cap > 0 else 0.0
            table.add_row(label, f"{value:.0f}", _bar(value, cap), f"{pct:.0%}")

    _row("Requests Running", snapshot.num_requests_running, 50.0)
    _row("Requests Waiting", snapshot.num_requests_waiting, 5.0)
    return Panel(
        table, title="[dim]Supporting Metrics[/dim]", border_style="dim", padding=(0, 1)
    )


def _bullet_panel(title: str, items: list[str], border_color: str = "dim") -> Panel:
    lines = Group(*[Text(f"  • {item}") for item in items])
    return Panel(
        lines, title=f"[dim]{title}[/dim]", border_style=border_color, padding=(0, 1)
    )


def _finding_sections(finding: Finding) -> list[RenderableType]:
    color = _SEVERITY_COLOR[finding.severity]
    label = _SEVERITY_LABEL[finding.severity]
    sections: list[RenderableType] = [
        Text.assemble(("Health: ", "bold"), (label, f"bold {color}")),
        Text.assemble(("Main issue: ", "bold"), finding.title),
        Text.assemble(("Confidence: ", "bold"), finding.confidence.value.capitalize()),
        Text(),
    ]
    if finding.evidence:
        sections.append(_bullet_panel("Evidence", finding.evidence))
        sections.append(Text())
    if finding.recommendations:
        sections.append(_bullet_panel("Recommendation", finding.recommendations))
        sections.append(Text())
    return sections


def render_text(
    findings: list[Finding],
    snapshot: MetricSnapshot,
    console: Console | None = None,
) -> None:
    console = console or Console()

    sections: list[RenderableType] = []

    if not findings:
        sections += [
            Text.assemble(("Health: ", "bold"), ("OK", "bold green")),
            Text(),
            Text("  No issues detected.", style="dim"),
            Text(),
        ]
    else:
        for i, finding in enumerate(findings):
            if i > 0:
                sections.append(Text())
            sections.extend(_finding_sections(finding))

    sections.append(_metrics_panel(snapshot))

    console.print(
        Panel(
            Group(*sections),
            title="[bold]vLLM Doctor[/bold]",
            title_align="center",
            border_style="dim",
            padding=(1, 2),
        )
    )
