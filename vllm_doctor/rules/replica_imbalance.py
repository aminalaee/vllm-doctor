"""
Replica imbalance rule.

Detects when load is unevenly distributed across the replicas of a deployment —
one replica overloaded while its peers sit idle. This points at the routing layer
rather than the model: an uneven load balancer, an unready replica receiving no
traffic, or long-context requests pinned to a subset of pods.

Because one vLLM deployment serves one model, replicas are grouped by the
`model_name` label and compared only against peers serving the same model. This
keeps the comparison correct on a shared Prometheus that scrapes several
deployments, with or without a model filter. A group with a single replica is
skipped (nothing to compare).

Signals (each matching signal increases confidence), evaluated per model group:
  - running spread: busiest replica handles >= imbalance_factor x the least busy
    (gated by a minimum total running load to avoid firing on noise)
  - cache gap: kv_cache_usage_perc max - min >= cache_gap
  - waiting skew: one replica has queued requests while another has none

Confidence:
  1 signal  -> low
  2 signals -> medium
  3 signals -> high
"""

import math

from vllm_doctor.config import ReplicaImbalanceConfig
from vllm_doctor.metrics import MetricSeries, MetricSeriesSnapshot, detect_replica_label
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule

_MODEL_LABEL = "model_name"


def _models(series: list[MetricSeries]) -> list[str | None]:
    values: set[str] = set()
    labeled = False
    for s in series:
        for sample in s.samples:
            if _MODEL_LABEL in sample.labels:
                labeled = True
                values.add(sample.labels[_MODEL_LABEL])
    return sorted(values) if labeled else [None]


def _per_replica(series: MetricSeries, model: str | None, label: str) -> dict[str, float]:
    scoped = series if model is None else series.filter(**{_MODEL_LABEL: model})
    return {k: v for k, v in scoped.by(label).items() if v is not None and math.isfinite(v)}


def _extremes(values: dict[str, float]) -> tuple[str, str]:
    return max(values, key=values.__getitem__), min(values, key=values.__getitem__)


class ReplicaImbalanceRule(Rule[ReplicaImbalanceConfig]):
    id = "replica_imbalance"
    name = "Replica Imbalance"
    title = "Replica imbalance"
    severity = Severity.warning
    config_attr = "replica_imbalance"
    config_cls = ReplicaImbalanceConfig
    likely_causes = [
        "Load balancer not distributing requests evenly (sticky sessions or connection reuse)",
        "A replica is not Ready or recently restarted, so traffic skips it",
        "Long-context requests pinned to a subset of replicas",
        "Autoscaler added replicas that are not yet receiving traffic",
    ]
    recommendations = [
        "Check the load balancer / service routing and session affinity settings",
        "Verify readiness probes — an unready replica receives no traffic",
        "Compare per-replica latency and restart any unhealthy replica",
        "Confirm newly added replicas are registered with the load balancer",
    ]
    related_metrics = [
        "vllm:num_requests_running",
        "vllm:num_requests_waiting",
        "vllm:kv_cache_usage_perc",
    ]

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        label = detect_replica_label(metrics)
        if label is None:
            return None

        running_series = metrics.num_requests_running
        waiting_series = metrics.num_requests_waiting
        cache_series = metrics.kv_cache_usage_perc

        evidence: list[str] = []
        signals: set[str] = set()
        worst_count = 0

        for model in _models([running_series, waiting_series, cache_series]):
            running = _per_replica(running_series, model, label)
            waiting = _per_replica(waiting_series, model, label)
            cache = _per_replica(cache_series, model, label)

            parts: list[str] = []
            count = 0

            if len(running) >= 2:
                hi, lo = _extremes(running)
                total = sum(running.values())
                if total >= self.cfg.min_total_running and (
                    (running[lo] > 0 and running[hi] >= self.cfg.imbalance_factor * running[lo])
                    or (running[lo] == 0 and running[hi] > 0)
                ):
                    count += 1
                    signals.add("Uneven running requests across replicas")
                    parts.append(f"running {hi}={running[hi]:.0f} vs {lo}={running[lo]:.0f}")

            if len(cache) >= 2:
                hi, lo = _extremes(cache)
                if cache[hi] - cache[lo] >= self.cfg.cache_gap:
                    count += 1
                    signals.add("Uneven KV cache usage across replicas")
                    parts.append(f"cache {cache[hi]:.0%} vs {cache[lo]:.0%}")

            if len(waiting) >= 2:
                hi, lo = _extremes(waiting)
                if waiting[hi] > 0 and waiting[lo] == 0:
                    count += 1
                    signals.add("Requests queued on some replicas while others are idle")
                    parts.append(f"waiting {hi}={waiting[hi]:.0f} vs {lo}={waiting[lo]:.0f}")

            if count > 0:
                prefix = f"{model}: " if model is not None else ""
                evidence.append(prefix + "; ".join(parts))
                worst_count = max(worst_count, count)

        if not evidence:
            return None

        confidence = Confidence.high if worst_count >= 3 else Confidence.medium if worst_count == 2 else Confidence.low

        if len(evidence) == 1:
            summary = "Load is unevenly distributed across replicas — one replica is doing more work than its peers."
        else:
            summary = f"{len(evidence)} models have load unevenly distributed across their replicas."

        return FindingData(
            confidence=confidence,
            summary=summary,
            signals=sorted(signals),
            evidence=evidence,
        )
