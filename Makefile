all: lint test

setup:
	uv sync --all-groups

test:
	uv run pytest tests/unit --cov=vllm_doctor --cov-report=term-missing --cov-fail-under=90

test-integration:
	uv run pytest tests/integration/ -v

lint:
	uv run ruff check vllm_doctor tests
	uv run ruff format --check vllm_doctor tests

format:
	uv run ruff format vllm_doctor tests
	uv run ruff check --fix vllm_doctor tests

demo:
	# requires: brew install charmbracelet/tap/freeze
	uv run python scripts/serve_metrics.py tests/fixtures/scrape/demo.txt > /dev/null 2>&1 & \
	sleep 0.5 && \
	{ printf '$$ vllm-doctor --url http://localhost:8000/metrics\n\n'; FORCE_COLOR=1 COLUMNS=120 uv run vllm-doctor --url http://localhost:8000/metrics; } \
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

.PHONY: all setup test test-integration lint format demo docs docs-build build publish
