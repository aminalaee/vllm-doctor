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
| `-s`, `--since`   | `now`   | Time window (e.g. `1h`, `30m`, `now`).                                                 |
| `-m`, `--model`   | —       | Filter metrics by `model_name` label. Useful when several models share one Prometheus. |
| `-w`, `--watch`   | False   | Refresh continuously every 5 seconds.                                                  |
| `-o`, `--output`  | `text`  | Output format: `text` or `json`.                                                       |
| `-v`, `--verbose` | False   | Show observed metrics and per-replica breakdown.                                       |
| `-c`, `--config`  | —       | Path to config file (default: `vllm-doctor.toml`).                                     |
| `--save`          | False   | Persist the run to the local database.                                                 |

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

## Save a run

Persist a one-shot diagnosis to the local database:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --save
```

## Watch with change-log

Combine `--save` and `--watch` to produce a change-log: a new run is saved only when the diagnosis state transitions (health changes or the set of firing rules changes).

```shell
vllm-doctor diagnose http://localhost:8000/metrics --save --watch
```

Save messages are printed to stderr so they do not corrupt text or JSON output.

## Filter by model

When a Prometheus target serves several models, aggregate metrics blend across them. Use `--model` to scope the diagnosis to one model.

```shell
vllm-doctor diagnose http://localhost:9090 --model meta-llama/Llama-3.1-8B
```
