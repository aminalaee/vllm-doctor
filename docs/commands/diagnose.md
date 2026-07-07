# `vllm-doctor diagnose`

Run a one-shot diagnosis or watch a target continuously.

## Usage

```shell
vllm-doctor diagnose [OPTIONS] URL
```

`URL` is a vLLM `/metrics` endpoint or a Prometheus server (e.g. `http://localhost:8000/metrics` or `http://localhost:9090`).

## Options

| Option            | Default | Description                                                                            |
| ----------------- | ------- | -------------------------------------------------------------------------------------- |
| `-s`, `--since`   | `now`   | Time window (e.g. `1h`, `30m`, `now` — `now` means last 5 minutes).                    |
| `-m`, `--model`   | —       | Filter metrics by `model_name` label. Useful when several models share one Prometheus. |
| `-w`, `--watch`   | False   | Refresh continuously every 5 seconds.                                                  |
| `-o`, `--output`  | `text`  | Output format: `text` or `json`.                                                       |
| `-v`, `--verbose` | False   | Show observed metrics and per-replica breakdown.                                       |
| `--save`          | False   | Persist this diagnosis run to the local database.                                      |
| `-c`, `--config`  | —       | Path to config file (default: `vllm-doctor.toml`).                                     |

For persistence and the watch change-log, see the [history guide](history.md).

## One-shot diagnosis

```shell
vllm-doctor diagnose http://localhost:8000/metrics
```

## JSON output

```shell
vllm-doctor diagnose http://localhost:8000/metrics --output json
```

## Watch mode

Refresh every 5 seconds until interrupted:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --watch
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
