from abc import ABC, abstractmethod

from vllm_doctor.models import DiagnosisContext, Finding, Metrics


class Rule(ABC):
    @property
    @abstractmethod
    def name(self) -> str: ...

    @abstractmethod
    def evaluate(
        self,
        context: DiagnosisContext,
        current: Metrics,
        previous: Metrics | None = None,
    ) -> list[Finding]: ...
