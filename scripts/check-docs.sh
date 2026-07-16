#!/usr/bin/env bash
# Guard against README/docs example blocks drifting from real CLI output.
#
# Serves the canonical mock fixture, renders `diagnose` (default + verbose),
# and fails if the docs no longer contain that output verbatim. Run in CI and
# locally after changing rules, templates, or the report renderer.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=./target/release/vllm-doctor
[ -x "$BIN" ] || cargo build --release --quiet

PORT=8765
python3 examples/mock/serve_metrics.py examples/mock/prometheus.json --port "$PORT" >/dev/null 2>&1 &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null || true' EXIT
sleep 1

if ! kill -0 "$SERVER" 2>/dev/null; then
    echo "Mock metrics server failed to start."
    exit 1
fi

# `diagnose` exits 1 on critical findings (the demo fixture is CRITICAL), so
# ignore its exit status here — we only compare the rendered output.
default=$(NO_COLOR=1 "$BIN" diagnose "http://localhost:$PORT" || true)
verbose=$(NO_COLOR=1 "$BIN" diagnose -v "http://localhost:$PORT" || true)

if [ -z "$default" ] || [ -z "$verbose" ]; then
    echo "CLI produced no documentation output."
    exit 1
fi

status=0
contains() { # file, block, label
    if python3 -c 'import sys; sys.exit(0 if sys.argv[2].strip() in open(sys.argv[1]).read() else 1)' "$1" "$2"; then
        echo "✓ $1 — $3 is current"
    else
        echo "✗ DRIFT: $1 — $3 does not match current CLI output"
        status=1
    fi
}

contains README.md "$default" "default example"
contains README.md "$verbose" "verbose example"
contains docs/index.md "$verbose" "verbose example"

if [ "$status" -ne 0 ]; then
    echo
    echo "Docs are out of date with the CLI. Update the example blocks (and 'make demo')."
    exit 1
fi
echo "All example blocks are current."
