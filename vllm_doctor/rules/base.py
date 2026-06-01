from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, ClassVar

from vllm_doctor.models import FindingData, Metrics, Severity

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
    def run(self, metrics: Metrics) -> FindingData | None: ...
