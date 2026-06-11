"""Serve a metrics fixture file over HTTP for local testing.

Usage:
    python scripts/serve_metrics.py tests/fixtures/scrape/kv-pressure.txt
    python scripts/serve_metrics.py tests/fixtures/prometheus/demo.json --port 9090

Then in another terminal:
    vllm-doctor diagnose http://localhost:8000 --verbose
"""

import http.server
import json
from collections.abc import Mapping
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import typer


def _prometheus_response(fixture: Mapping[str, list[dict]], query: str) -> bytes:
    result = next((value for key, value in fixture.items() if key in query), [])
    body = {"status": "success", "data": {"resultType": "vector", "result": result}}
    return json.dumps(body).encode()


def main(
    fixture: Path = typer.Argument(..., help="Path to the fixture .txt or .json file."),
    port: int = typer.Option(8000, help="Port to listen on."),
) -> None:
    if fixture.suffix == ".json":
        prometheus_fixture = json.loads(fixture.read_text())
        scrape_content = None
    else:
        prometheus_fixture = None
        scrape_content = fixture.read_bytes()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if prometheus_fixture is not None and self.path.startswith("/api/v1/query"):
                parsed = urlparse(self.path)
                query = parse_qs(parsed.query).get("query", [""])[0]
                content = _prometheus_response(prometheus_fixture, query)
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)
            elif scrape_content is not None and self.path == "/metrics":
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; version=0.0.4")
                self.send_header("Content-Length", str(len(scrape_content)))
                self.end_headers()
                self.wfile.write(scrape_content)
            else:
                self.send_response(404)
                self.end_headers()

        def log_message(self, fmt: str, *args: object) -> None:
            pass  # suppress request logs

    server = http.server.HTTPServer(("", port), Handler)
    url = f"http://localhost:{port}" if prometheus_fixture is not None else f"http://localhost:{port}/metrics"
    typer.echo(f"Serving {fixture} on {url}")
    typer.echo(f"Run: vllm-doctor diagnose {url} --verbose")
    typer.echo("Press Ctrl+C to stop.")
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == "__main__":
    typer.run(main)
