all: lint test

setup:
	cargo build

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

format:
	cargo fmt

demo:
	# requires: brew install charmbracelet/tap/freeze
	python3 examples/mock/serve_metrics.py examples/mock/prometheus.json > /dev/null 2>&1 & \
	sleep 0.5 && \
	{ printf '$$ vllm-doctor diagnose http://localhost:8000\n\n'; NO_COLOR= FORCE_COLOR=1 CLICOLOR_FORCE=1 TERM=xterm-256color COLUMNS=120 ./target/release/vllm-doctor diagnose http://localhost:8000; } \
		| freeze - --language ansi --output docs/demo.png --window --shadow.blur 20 --shadow.x 0 --shadow.y 8; \
	kill %1 2>/dev/null || true

build:
	cargo build --release

docs:
	pip install zensical~=0.0.42 2>/dev/null; python3 -m zensical serve

docs-build:
	pip install zensical~=0.0.42 2>/dev/null; python3 -m zensical build

.PHONY: all setup test lint format demo build docs docs-build
