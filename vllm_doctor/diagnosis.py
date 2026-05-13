from collections.abc import Sequence

from vllm_doctor.models import Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule

_SEVERITY_ORDER = {Severity.critical: 0, Severity.warning: 1, Severity.info: 2}


def run(snapshot: MetricSnapshot, rules: Sequence[Rule]) -> list[Finding]:
    findings = [f for rule in rules for f in rule.evaluate(snapshot)]
    return sorted(findings, key=lambda f: _SEVERITY_ORDER[f.severity])
