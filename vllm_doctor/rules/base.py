from abc import ABC, abstractmethod

from vllm_doctor.models import Finding, MetricSnapshot


class Rule(ABC):
    @property
    @abstractmethod
    def name(self) -> str: ...

    @abstractmethod
    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]: ...
