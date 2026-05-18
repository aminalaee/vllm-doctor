from rich.console import Console
from rich.rule import Rule
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

    if finding.evidence:
        console.print(Text(f"  {'  ·  '.join(finding.evidence)}", style="dim"))
        console.print()

    if finding.recommendations:
        for r in finding.recommendations:
            console.print(Text.assemble(("  → ", f"bold {color}"), (r, "")))
        console.print()


def _print_metrics(result: DiagnosisResult, console: Console) -> None:
    for name, field in Metrics.model_fields.items():
        value = getattr(result.snapshot.metrics, name)
        if value is None:
            continue
        label = field.title or name
        fmt = str((field.json_schema_extra or {}).get("fmt", ".0f"))
        formatted = format(value, fmt)
        console.print(Text(f"  {label:<24}{formatted:>8}"))


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
            _print_finding(finding, console)

    if verbose:
        console.print(Rule("Observed Metrics", style="dim"))
        console.print()
        _print_metrics(result, console)
        console.print()
