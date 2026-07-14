# Mock metrics

This example serves predefined metrics so you can try vLLM Doctor without
installing vLLM or loading a model. It mocks only the metrics source, not the
vLLM inference API.

## Prometheus API

Start the mock Prometheus server:

```shell
python3 examples/mock/serve_metrics.py \
  examples/mock/prometheus.json --port 9090
```

In another terminal:

```shell
vllm-doctor diagnose http://localhost:9090 --verbose
```

## Direct `/metrics` scrape

Serve the mock `/metrics` endpoint:

```shell
python3 examples/mock/serve_metrics.py \
  examples/mock/metrics.txt
```

In another terminal:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --verbose
```

## What the files contain

`prometheus.json` is the combined documentation demo, including multiple
replicas and historical latency data. `metrics.txt` is a representative raw
vLLM scrape. Prometheus supports historical rates and latency percentiles; a
direct scrape only exposes the current values and cumulative counters available
at `/metrics`.
