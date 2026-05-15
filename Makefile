all: lint test

setup:
	uv sync --all-groups

test:
	uv run pytest --cov=vllm_doctor --cov-report=term-missing

test-integration:
	uv run pytest tests/integration/ -v

lint:
	uv run ruff check vllm_doctor tests
	uv run ruff format --check vllm_doctor tests

format:
	uv run ruff format vllm_doctor tests
	uv run ruff check --fix vllm_doctor tests

docs:
	uv run mkdocs serve

docs-build:
	uv run mkdocs build --strict

build:
	uv build

.PHONY: all setup test test-integration lint format docs docs-build build
