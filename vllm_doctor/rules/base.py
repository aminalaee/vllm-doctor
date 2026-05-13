from abc import ABC, abstractmethod

from vllm_doctor.models import Finding, MetricSnapshot


class Rule(ABC):
    @abstractmethod
    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]: ...
