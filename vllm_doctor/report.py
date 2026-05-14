from rich.console import Console
from rich.rule import Rule
from rich.text import Text

from vllm_doctor.models import Finding, MetricSnapshot, Severity

_SEVERITY_COLOR = {
    Severity.critical: "red",
    Severity.warning: "yellow",
    Severity.info: "blue",
}

_SEVERITY_ICON = {
    Severity.critical: "✖",
    Severity.warning: "⚠",
    Severity.info: "ℹ",
}


def _print_finding(finding: Finding, console: Console) -> None:
    color = _SEVERITY_COLOR[finding.severity]
    icon = _SEVERITY_ICON[finding.severity]

    console.print(
        Text.assemble(
            (f"{icon} ", f"bold {color}"),
            (finding.title, "bold"),
            (f"  [{finding.confidence.value} confidence]", "dim"),
        )
    )

    if finding.signals:
        for s in finding.signals:
            console.print(Text(f"  {s}", style="dim"))
        console.print()

    if finding.evidence:
        for e in finding.evidence:
            console.print(Text(f"  {e}"))
        console.print()

    if finding.recommendations:
        for r in finding.recommendations:
            console.print(Text.assemble(("  → ", f"bold {color}"), (r, "")))
        console.print()


def _print_metrics(snapshot: MetricSnapshot, console: Console) -> None:
    def _row(label: str, value: float | None, fmt: str = ".0f") -> None:
        val = "n/a" if value is None else f"{value:{fmt}}"
        console.print(Text(f"  {label:<24}{val:>8}"))

    _row("Requests Running", snapshot.num_requests_running)
    _row("Requests Waiting", snapshot.num_requests_waiting)
    _row("GPU Cache Usage", snapshot.gpu_cache_usage_perc, fmt=".0%")


def render_text(
    findings: list[Finding],
    snapshot: MetricSnapshot,
    console: Console | None = None,
) -> None:
    console = console or Console()

    if not findings:
        health = Text.assemble(("Health: ", "bold"), ("OK", "bold green"))
    else:
        worst = min(findings, key=lambda f: list(Severity).index(f.severity))
        color = _SEVERITY_COLOR[worst.severity]
        health = Text.assemble(
            ("Health: ", "bold"), (worst.severity.value.upper(), f"bold {color}")
        )

    console.print(
        Rule(
            Text.assemble(
                ("vLLM Doctor", "bold"),
                ("  ·  ", "dim"),
                health,
                ("  ·  ", "dim"),
                Text(f"Window: {snapshot.window}", style="dim"),
            ),
            style="dim",
        )
    )
    console.print()

    if not findings:
        console.print(Text("  No issues detected.", style="green"))
        console.print()
    else:
        for finding in findings:
            _print_finding(finding, console)

    console.print(Rule("Observed Metrics", style="dim"))
    console.print()
    _print_metrics(snapshot, console)
    console.print()
