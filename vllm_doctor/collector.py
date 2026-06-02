from dataclasses import dataclass

from vllm_doctor.clients import Client
from vllm_doctor.metrics import METRIC_SPECS, Metrics, MetricSeriesSnapshot
from vllm_doctor.probes import run_probes


@dataclass(frozen=True)
class MetricCollection:
    series: MetricSeriesSnapshot

    @property
    def metrics(self) -> Metrics:
        return self.series.to_metrics()


async def collect(
    client: Client,
    since: str,
    model: str | None = None,
) -> MetricCollection:
    if since == "now":
        since = "5m"
    needed = {name for spec in METRIC_SPECS for name in spec.probe_names()}
    raw = await run_probes(client, needed, since, model)
    series = MetricSeriesSnapshot(**{spec.output: spec.compute(raw) for spec in METRIC_SPECS})
    return MetricCollection(series=series)
