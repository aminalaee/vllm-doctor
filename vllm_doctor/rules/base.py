from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, ClassVar

from vllm_doctor.models import DiagnosisContext, Finding, FindingData, Metrics, Severity

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig


class Rule(ABC):
    name: ClassVar[str]
    title: ClassVar[str]
    severity: ClassVar[Severity]
    likely_causes: ClassVar[list[str]] = []
    recommendations: ClassVar[list[str]] = []
    related_metrics: ClassVar[list[str]] = []

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "Rule":
        raise NotImplementedError(f"{cls.__name__}.from_config is not implemented")

    @abstractmethod
    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None: ...

    def run(
        self,
        context: DiagnosisContext,
        current: Metrics,
        previous: Metrics | None = None,
    ) -> list[Finding]:
        result = self._run(current, previous)
        if result is None:
            return []
        return [
            Finding(
                severity=result.severity if result.severity is not None else self.severity,
                confidence=result.confidence,
                title=self.title,
                summary=result.summary,
                signals=result.signals,
                evidence=result.evidence,
                likely_causes=self.likely_causes,
                recommendations=self.recommendations,
                related_metrics=self.related_metrics,
            )
        ]
