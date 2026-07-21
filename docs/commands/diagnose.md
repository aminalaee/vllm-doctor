# `vllm-doctor diagnose`

Run a one-shot diagnosis or watch a target continuously.

## Usage

```shell
vllm-doctor diagnose [OPTIONS] URL
```

`URL` is a vLLM `/metrics` endpoint or a Prometheus server (e.g. `http://localhost:8000/metrics` or `http://localhost:9090`).

## Options

| Option             | Default | Description                                                                            |
| ------------------ | ------- | -------------------------------------------------------------------------------------- |
| `-s`, `--since`    | `now`   | Time window (e.g. `1h`, `30m`, `now` — `now` means last 5 minutes).                    |
| `-m`, `--model`    | —       | Filter metrics by `model_name` label. Useful when several models share one Prometheus. |
| `-w`, `--watch`    | False   | Refresh continuously until interrupted (interval set by `--interval`).                 |
| `-i`, `--interval` | `5`     | Base seconds between watch refreshes; actual polling is jittered by ±20%.              |
| `--listen`         | —       | Serve Doctor health and Prometheus metrics at `ADDR` during watch mode.                |
| `-o`, `--output`   | `text`  | Output format: `text` or `json`.                                                       |
| `-v`, `--verbose`  | False   | Show full evidence, recommendations, observed metrics, and per-replica breakdown.      |
| `--save`           | False   | Persist this diagnosis run to the local database.                                      |
| `-t`, `--timeout`  | `10`    | HTTP request timeout in seconds. Raise it for slow or overloaded targets.              |
| `--header`         | —       | Extra HTTP header to send with every request (`NAME:VALUE`, repeatable).               |
| `--ca-cert`        | —       | Path to a PEM file containing a CA certificate to trust.                               |
| `-c`, `--config`   | —       | Path to config file (default: `vllm-doctor.toml`).                                     |

For persistence and the watch change-log, see the [history guide](history.md).

## Default vs verbose output

By default `diagnose` prints a compact triage summary: the most likely bottleneck, one line per firing finding, and a count of passing checks. Use `--verbose` (`-v`) for the full detail — evidence, recommended actions, observed metrics tables, and any notices.

```shell
# Compact (default)
vllm-doctor diagnose http://localhost:8000/metrics

# Full detail
vllm-doctor diagnose http://localhost:8000/metrics --verbose
```

## One-shot diagnosis

```shell
vllm-doctor diagnose http://localhost:8000/metrics
```

## Try it locally

The repository includes two runnable examples:

- [Mock metrics](https://github.com/vllm-doctor/vllm-doctor/tree/main/examples/mock)
  exercises predefined diagnoses without requiring vLLM.
- [Live vLLM](https://github.com/vllm-doctor/vllm-doctor/tree/main/examples/live-vllm)
  starts a small real model with vLLM.

## JSON output

```shell
vllm-doctor diagnose http://localhost:8000/metrics --output json
```

## Watch mode

Refresh until interrupted. The default interval is 5 seconds; change it with `--interval`:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --watch
vllm-doctor diagnose http://localhost:8000/metrics --watch --interval 2
```

Watch mode adds bounded jitter to polling so multiple Doctor processes do not query at exactly the same time. Initial connection and collection failures are logged to stderr and retried with exponential backoff starting at 1 second and capped at 60 seconds. The backoff resets after the next successful diagnosis.

Ctrl-C or SIGTERM cancels both an in-progress collection and any polling or retry sleep. Interactive text output continues to redraw the terminal. When stdout is redirected, reports are appended without terminal-clear escape sequences. JSON watch output emits one complete JSON object for each successful iteration.

Retry timing is intentionally not configurable yet.

### Monitor Doctor itself

Add `--listen` to expose process health, target readiness, and Doctor's own Prometheus metrics while watch mode runs:

```shell
vllm-doctor diagnose http://localhost:8000/metrics \
  --watch --listen 127.0.0.1:9091

curl http://127.0.0.1:9091/healthz
curl http://127.0.0.1:9091/readyz
curl http://127.0.0.1:9091/metrics
```

`/healthz` confirms that the HTTP task is alive. `/readyz` is ready only when the latest target collection succeeded. The listener is disabled unless `--listen` or `[agent].listen` is configured. These endpoints have no authentication or TLS; only bind them to a trusted interface.

The `/metrics` endpoint publishes five Prometheus metric families. Every sample includes `target` and `engine`; `vllm_doctor_finding` also includes `rule` and `severity`.

| Metric                                       | Type    | Meaning                                                                               |
| -------------------------------------------- | ------- | ------------------------------------------------------------------------------------- |
| `vllm_doctor_ready`                          | Gauge   | `1` when the latest collection succeeded, otherwise `0`.                              |
| `vllm_doctor_target_health`                  | Gauge   | Last known health: `-1` unknown, `0` healthy, `1` info, `2` warning, or `3` critical. |
| `vllm_doctor_last_success_timestamp_seconds` | Gauge   | Unix timestamp of the last successful diagnosis, or `0` before the first success.     |
| `vllm_doctor_collection_errors_total`        | Counter | Provider setup and collection failures since Doctor started.                          |
| `vllm_doctor_finding`                        | Gauge   | One sample with value `1` per finding from the last successful diagnosis.             |

During a collection failure, health and findings retain the last successful diagnosis. Use `vllm_doctor_ready` to determine whether they are fresh.

## Save runs

Persist a diagnosis run to the local database. Combine with `--watch` to log only on state transitions:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --save
vllm-doctor diagnose http://localhost:8000/metrics --watch --save
```

Review saved runs with [`vllm-doctor history`](history.md).

## Filter by model

When a Prometheus target serves several models, aggregate metrics blend across them. Use `--model` to scope the diagnosis to one model.

```shell
vllm-doctor diagnose http://localhost:9090 --model meta-llama/Llama-3.1-8B
```

## Authenticated endpoints

Pass `--header` for any HTTP header — covers bearer tokens, basic auth, and custom headers. Repeat the flag for multiple headers:

```shell
vllm-doctor diagnose https://prometheus.internal:9090 \
  --header "Authorization: Bearer $TOKEN"
```

For endpoints using a self-signed or internal CA, pass the certificate bundle:

```shell
vllm-doctor diagnose https://prometheus.internal:9090 \
  --header "Authorization: Bearer $TOKEN" \
  --ca-cert /etc/ssl/certs/internal-ca.pem
```

## Exit codes

`diagnose` follows the convention used by linters like Ruff — the result is reflected in the exit code so it can gate CI and scripts:

| Code | Meaning                                              |
| ---- | ---------------------------------------------------- |
| `0`  | Ran successfully; health is not critical.            |
| `1`  | Ran successfully, but a critical finding fired.      |
| `2`  | Operational error (could not reach or read metrics). |

This lets a pipeline distinguish "vLLM is critically unhealthy" (`1`) from "the tool itself failed" (`2`):

```shell
vllm-doctor diagnose http://localhost:8000/metrics
case $? in
  0) echo "healthy" ;;
  1) echo "critical finding — alert" ;;
  2) echo "tool error — could not diagnose" ;;
esac
```

Watch mode runs until interrupted and does not gate on health.
