from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, ClassVar, Generic, TypeVar, cast

from pydantic import BaseModel

from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import FindingData, Severity

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig


class _NoConfig(BaseModel):
    """Default config for rules that have no thresholds."""


C = TypeVar("C", bound=BaseModel)


class Rule(ABC, Generic[C]):
    id: ClassVar[str]
    name: ClassVar[str]
    title: ClassVar[str]
    severity: ClassVar[Severity]
    likely_causes: ClassVar[list[str]] = []
    recommendations: ClassVar[list[str]] = []
    related_metrics: ClassVar[list[str]] = []
    config_attr: ClassVar[str] = ""
    config_cls: ClassVar[type[BaseModel]] = _NoConfig

    def __init__(self, **overrides: object) -> None:
        self.cfg: C = cast(C, self.config_cls(**overrides))

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "Rule[C]":
        return cls(**getattr(config, cls.config_attr).model_dump())

    @abstractmethod
    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None: ...
