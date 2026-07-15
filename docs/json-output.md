# JSON Output

`--output json` is the stable machine-readable output for automation.

Text output is optimized for terminal use and is not treated as a stable report format.

## Examples

Default JSON output:

```json
{
  "schema_version": "1",
  "metadata": {
    "generated_at": "2026-06-01T10:30:00+00:00",
    "target": {
      "model_name": "meta-llama/Llama-3.1-8B",
      "since": "5m",
      "client_mode": "prometheus"
    }
  },
  "health": "warning",
  "assessment": {
    "likely_bottleneck": "kv_cache_saturation",
    "confidence": "high",
    "evidence": [
      {
        "kind": "threshold",
        "metric": "kv_cache_usage_perc",
        "value": 0.94,
        "threshold": 0.90,
        "operator": "greater_than_or_equal"
      },
      {
        "kind": "value",
        "metric": "num_requests_waiting",
        "value": 7
      }
    ],
    "interpretation": "Requests are likely waiting because the server has limited KV cache headroom, often caused by high concurrency or long-context requests.",
    "recommended_next_actions": [
      "Check max_num_seqs and max_num_batched_tokens",
      "Route long-context traffic separately"
    ]
  },
  "notices": [],
  "checks": [
    {
      "id": "replica_imbalance",
      "name": "Replica Imbalance",
      "finding": {
        "severity": "warning",
        "confidence": "high",
        "title": "Replica imbalance",
        "evidence": [
          {
            "kind": "replica_distribution",
            "affected": 1,
            "total": 2,
            "metric": "num_requests_running"
          }
        ],
        "likely_causes": ["Load balancer not distributing requests evenly (sticky sessions or connection reuse)"],
        "recommendations": ["Check the load balancer / service routing and session affinity settings"],
        "related_metrics": ["vllm:num_requests_running"]
      }
    },
    {
      "id": "queue_pressure",
      "name": "Queue Pressure",
      "finding": {
        "severity": "warning",
        "confidence": "low",
        "title": "Queue pressure",
        "evidence": [
          {
            "kind": "threshold",
            "metric": "num_requests_waiting",
            "value": 7,
            "threshold": 5,
            "operator": "greater_than"
          }
        ],
        "likely_causes": ["Insufficient replica capacity for current traffic"],
        "recommendations": ["Add replicas or increase concurrency limits"],
        "related_metrics": ["vllm:num_requests_waiting"]
      }
    }
  ]
}
```

When several deployments share one Prometheus target, replica imbalance emits a `replica_distribution` evidence item per affected model. The optional `model` field identifies the deployment while `metric` remains a stable metric identifier.

Verbose JSON includes observed metrics:

```json
{
  "schema_version": "1",
  "metadata": {
    "generated_at": "2026-06-01T10:30:00+00:00",
    "target": {
      "model_name": null,
      "since": "5m",
      "client_mode": "prometheus"
    }
  },
  "health": "warning",
  "notices": [],
  "checks": [],
  "metrics": {
    "num_requests_running": {
      "value": 12,
      "by": {
        "pod": {
          "vllm-0": 2,
          "vllm-1": 10
        }
      }
    },
    "kv_cache_usage_perc": {
      "value": 0.94,
      "by": {
        "pod": {
          "vllm-0": 0.41,
          "vllm-1": 0.94
        }
      }
    }
  }
}
```

!!! note
    `--output json` (one-shot) produces pretty-printed JSON for readability. `--output json --watch` produces compact JSON (one object per line) for streaming and automation.

## Top-level fields

| Field            | Description                                                                                                 |
| ---------------- | ----------------------------------------------------------------------------------------------------------- |
| `schema_version` | JSON schema version. Current value: `1`.                                                                    |
| `metadata`       | Report metadata, including generation time and target                                                       |
| `health`         | Overall health: `healthy`, `info`, `warning`, `critical`                                                    |
| `assessment`     | Root-cause summary: the single most likely bottleneck (see [Assessment](assessment.md))                     |
| `notices`        | Advisory caveats about reading the report (scrape-mode limits, multi-model blending); empty when none apply |
| `checks`         | Rule results, sorted by severity and confidence                                                             |
| `metrics`        | Observed metrics; included only with `--verbose`                                                            |

## Metrics

`value` is the scalar diagnostic value for the metric. `by` is present only when vLLM Doctor detects multiple replicas — its single key is the label that distinguishes them (e.g. `pod`, `instance`, `host`, `server`), and the inner map keys are the per-replica values. Per-replica numbers use the metric's own aggregation (`max` for KV cache usage and latency percentiles, `sum` for counters).

## Metadata

| Field                | Description                                      |
| -------------------- | ------------------------------------------------ |
| `generated_at`       | ISO 8601 timestamp for when the report was built |
| `target.model_name`  | Model name filter, or `null`                     |
| `target.since`       | Query window used for Prometheus rates           |
| `target.client_mode` | `prometheus` or `scrape`                         |

## Assessment

The `assessment` object is the root-cause summary. See [Assessment](assessment.md) for how it is derived.

| Field                      | Description                                                                                                                                                                       |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `likely_bottleneck`        | Machine token for the category: `queue_saturation`, `kv_cache_saturation`, `long_prefill`, `decode_bottleneck`, `replica_imbalance`, `error_issue`, `idle`, `no_clear_bottleneck` |
| `confidence`               | `low`, `medium`, or `high`                                                                                                                                                        |
| `evidence`                 | Structured evidence items (same format as finding evidence) drawn from the findings that fired this run |
| `interpretation`           | One-line human-readable explanation                                                                                                                                               |
| `recommended_next_actions` | Ordered list of safe next steps                                                                                                                                                   |

## Checks

Each check has a stable machine-readable `id` and a human-readable `name`.

| Field     | Description                                    |
| --------- | ---------------------------------------------- |
| `id`      | Stable rule ID, such as `queue_pressure`       |
| `name`    | Display name, such as `Queue Pressure`         |
| `finding` | Finding details, or `null` when the rule is OK |

Finding fields:

| Field        | Description                                                                           |
| ------------ | ------------------------------------------------------------------------------------- |
| `severity`   | `info`, `warning`, or `critical`                                                      |
| `confidence` | `low`, `medium`, or `high`                                                            |
| `title`      | Human-readable finding title                                                          |
| `evidence`   | Structured observed evidence for the finding. Each item has a `kind` and typed fields |
| `likely_causes`   | Possible causes to investigate                                                   |
| `recommendations` | Suggested next actions                                                           |
| `related_metrics` | Metrics related to the finding                                                   |

Evidence items:

| `kind`                 | Fields                                                                                            | Rendered meaning                                             |
| ---------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `threshold`            | `metric`, `value`, `threshold`, `operator`, optional `unit`                                       | A measured value crossed a threshold                         |
| `value`                | `metric`, `value`, optional `unit`                                                                | A simple metric reading                                      |
| `ratio`                | `numerator_metric`, `numerator_value`, `denominator_metric`, `denominator_value`, optional `unit` | A computed ratio of two metrics                              |
| `replica_distribution` | `affected`, `total`, `metric`, optional `model`                                                   | `affected` out of `total` replicas are elevated for `metric` |
| `text`                 | `message`                                                                                         | Free-form detail when no typed variant fits                  |

`operator` is one of `greater_than`, `greater_than_or_equal`, `less_than`, `less_than_or_equal`. Values are plain numbers when no `unit` is present; percentages are expressed as decimals (e.g. `0.94` for 94%).

`signals` are intentionally omitted from JSON findings for now. They remain internal explanatory detail and may be exposed in a later schema version.
