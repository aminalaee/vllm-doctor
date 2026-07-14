# Live vLLM with vLLM-Metal

Run a real vLLM-Metal server with a small model, then diagnose the metrics it
exposes. This example is intended for macOS and assumes vLLM-Metal is already
installed.

If vLLM-Metal is installed in a virtual environment, activate it first. For
example:

```shell
source ~/.venv-vllm-metal/bin/activate
```

Start the server:

```shell
examples/live-vllm/run-macos-metal.sh
```

The default model is `Qwen/Qwen3-0.6B`, the port is `8000`, and the maximum
model length is `512`. The first run may download the model, and Metal kernel
warm-up can take about a minute. Wait for vLLM to report that application
startup is complete.

In another terminal, diagnose the live endpoint:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --verbose
```

Override the defaults with environment variables:

```shell
MODEL=Qwen/Qwen3-0.6B PORT=8001 MAX_MODEL_LEN=1024 \
  examples/live-vllm/run-macos-metal.sh
```

The loopback host override in the launcher prevents vLLM's local worker from
selecting a VPN or another unsuitable network interface. Direct `/metrics`
diagnosis supports current gauges and cumulative counters. Connect vLLM to
Prometheus when you need historical rates and latency percentiles.
