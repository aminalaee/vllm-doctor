all: lint test

setup:
	uv sync --all-groups

test:
	uv run pytest tests --cov=vllm_doctor --cov-report=term-missing --cov-fail-under=95

lint:
	uv run ruff check vllm_doctor tests
	uv run ruff format --check vllm_doctor tests

format:
	uv run ruff format vllm_doctor tests
	uv run ruff check --fix vllm_doctor tests

demo:
	# requires: brew install charmbracelet/tap/freeze
	uv run python scripts/serve_metrics.py tests/fixtures/prometheus/demo.json > /dev/null 2>&1 & \
	sleep 0.5 && \
	{ printf '$$ vllm-doctor --url http://localhost:8000 --verbose\n\n'; NO_COLOR= FORCE_COLOR=1 CLICOLOR_FORCE=1 TERM=xterm-256color COLUMNS=120 uv run vllm-doctor --url http://localhost:8000 --verbose; } \
		| freeze - --language ansi --output docs/demo.png --window --shadow.blur 20 --shadow.x 0 --shadow.y 8; \
	kill %1 2>/dev/null || true

docs:
	uv run zensical serve

docs-build:
	uv run zensical build

build:
	uv build

publish:
	uv publish

.PHONY: all setup test lint format demo docs docs-build build publish
