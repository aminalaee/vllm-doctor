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
  "notice": null,
  "checks": [
    {
      "id": "queue_pressure",
      "name": "Queue Pressure",
      "finding": {
        "severity": "warning",
        "confidence": "low",
        "title": "Queue pressure",
        "summary": "Requests are queuing faster than the server can process them.",
        "evidence": ["Waiting requests: 7"],
        "likely_causes": ["Insufficient replica capacity for current traffic"],
        "recommendations": ["Add replicas or increase concurrency limits"],
        "related_metrics": ["vllm:num_requests_waiting"]
      }
    }
  ]
}
```

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
  "notice": null,
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

| Field            | Description                                           |
| ---------------- | ----------------------------------------------------- |
| `schema_version` | JSON schema version. Current value: `1`.              |
| `metadata`       | Report metadata, including generation time and target |
| `health`         | Overall health: `ok`, `info`, `warning`, `critical`   |
| `notice`         | Optional mode-specific notice                         |
| `checks`         | Rule results, sorted by severity and confidence       |
| `metrics`        | Observed metrics; included only with `--verbose`      |

## Metrics

`value` is the scalar diagnostic value for the metric. `by` is present only when vLLM Doctor detects multiple replicas — its single key is the label that distinguishes them (e.g. `pod`, `instance`, `host`, `server`), and the inner map keys are the per-replica values. Per-replica numbers use the metric's own aggregation (`max` for KV cache usage and latency percentiles, `sum` for counters).

## Metadata

| Field                | Description                                      |
| -------------------- | ------------------------------------------------ |
| `generated_at`       | ISO 8601 timestamp for when the report was built |
| `target.model_name`  | Model name filter, or `null`                     |
| `target.since`       | Query window used for Prometheus rates           |
| `target.client_mode` | `prometheus` or `scrape`                         |

## Checks

Each check has a stable machine-readable `id` and a human-readable `name`.

| Field     | Description                                    |
| --------- | ---------------------------------------------- |
| `id`      | Stable rule ID, such as `queue_pressure`       |
| `name`    | Display name, such as `Queue Pressure`         |
| `finding` | Finding details, or `null` when the rule is OK |

Finding fields:

| Field             | Description                             |
| ----------------- | --------------------------------------- |
| `severity`        | `info`, `warning`, or `critical`        |
| `confidence`      | `low`, `medium`, or `high`              |
| `title`           | Human-readable finding title            |
| `summary`         | Short explanation of the diagnosis      |
| `evidence`        | Observed signals supporting the finding |
| `likely_causes`   | Possible causes to investigate          |
| `recommendations` | Suggested next actions                  |
| `related_metrics` | Metrics related to the finding          |

`signals` are intentionally omitted from JSON findings for now. They remain internal explanatory detail and may be exposed in a later schema version.
