import math

from rich.console import Console, Group
from rich.panel import Panel
from rich.rule import Rule
from rich.table import Table
from rich.text import Text

from vllm_doctor.metrics import METRIC_SPECS, detect_replica_label
from vllm_doctor.models import DiagnosisResult, Finding, Health, Severity
from vllm_doctor.reports.notices import resolve_notices

_SEVERITY_COLOR = {
    Severity.critical: "red",
    Severity.warning: "yellow",
    Severity.info: "blue",
}

HEALTH_COLOR = {
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
_MAX_REPLICAS = 6


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

    for spec in METRIC_SPECS:
        name = spec.output
        value = getattr(result.metrics, name)
        if value is None:
            continue
        display = spec.display

        if display.bar:
            if not math.isfinite(value):
                table.add_row(display.title, Text("n/a", style="dim"))
            else:
                color = "red" if value >= 0.9 else "yellow" if value >= 0.7 else "green"
                table.add_row(display.title, _cache_bar(value, color))
        else:
            table.add_row(display.title, Text(format(value, display.fmt), style="bold"))

    return table


def _replica_table(result: DiagnosisResult, label: str) -> Table:
    values_per_spec: dict[str, dict[str, float | None]] = {}
    specs_with_data = []
    for spec in METRIC_SPECS:
        breakdown = getattr(result.metric_series, spec.output).by(label)
        if breakdown:
            specs_with_data.append(spec)
            values_per_spec[spec.output] = breakdown

    replicas = sorted({name for breakdown in values_per_spec.values() for name in breakdown})

    def sort_key(replica: str) -> tuple[float, float, float]:
        waiting = values_per_spec.get("num_requests_waiting", {}).get(replica) or 0.0
        cache = values_per_spec.get("kv_cache_usage_perc", {}).get(replica) or 0.0
        running = values_per_spec.get("num_requests_running", {}).get(replica) or 0.0
        return (waiting, cache, running)

    replicas = sorted(replicas, key=sort_key, reverse=True)
    visible = replicas[:_MAX_REPLICAS]
    hidden = len(replicas) - len(visible)

    table = Table(show_header=True, box=None, padding=(0, 2))
    table.add_column("", style="dim", no_wrap=True, max_width=28, overflow="ellipsis")
    for replica in visible:
        table.add_column(replica, justify="right", no_wrap=True)
    if hidden > 0:
        table.add_column(f"+{hidden} more", justify="right", no_wrap=True, style="dim")

    for spec in specs_with_data:
        row = [spec.display.title]
        for replica in visible:
            value = values_per_spec[spec.output].get(replica)
            row.append("n/a" if value is None or not math.isfinite(value) else format(value, spec.display.fmt))
        if hidden > 0:
            row.append("")
        table.add_row(*row)

    return table


def _observed_metrics(result: DiagnosisResult) -> list:
    items: list = [
        Rule("Observed Metrics", style="dim"),
        Text(),
        Text("  Summary", style="dim"),
        _metrics_table(result),
    ]
    label = detect_replica_label(result.metric_series)
    if label is not None:
        items += [Text(), Rule(f"Observed Metrics per {label}", style="dim"), Text(), _replica_table(result, label)]
    items.append(Text())
    return items


def build(result: DiagnosisResult, verbose: bool = False) -> Group:
    h = result.health
    color = HEALTH_COLOR[h]
    health = Text.assemble(("Health: ", "bold"), (h.value.upper(), f"bold {color}"))

    items: list = [
        Rule(
            Text.assemble(
                ("vLLM Doctor", "bold"),
                ("  ·  ", "dim"),
                health,
                ("  ·  ", "dim"),
                Text(f"Since: {result.context.since}", style="dim"),
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

    notices = resolve_notices(result)
    for notice in notices:
        items.append(Text(f"⚠ {notice}", style="dim yellow"))
    if notices:
        items.append(Text())

    if verbose:
        items += _observed_metrics(result)

    return Group(*items)


def render(result: DiagnosisResult, console: Console | None = None, verbose: bool = False) -> None:
    (console or Console()).print(build(result, verbose))
