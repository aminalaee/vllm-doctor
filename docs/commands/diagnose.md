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
| `-i`, `--interval` | `5`     | Seconds between refreshes in `--watch` mode. Only applies with `--watch`.              |
| `-o`, `--output`   | `text`  | Output format: `text` or `json`.                                                       |
| `-v`, `--verbose`  | False   | Show full evidence, recommendations, observed metrics, and per-replica breakdown. |
| `--save`           | False   | Persist this diagnosis run to the local database.                                      |
| `-t`, `--timeout`  | `10`    | HTTP request timeout in seconds. Raise it for slow or overloaded targets.              |
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
