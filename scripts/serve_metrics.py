"""Serve a Prometheus metrics fixture file over HTTP for local testing.

Usage:
    python scripts/serve_metrics.py tests/fixtures/metrics/kv-pressure.txt
    python scripts/serve_metrics.py tests/fixtures/metrics/kv-pressure.txt --port 9090

Then in another terminal:
    vllm-doctor --url http://localhost:8000/metrics --verbose
"""

import http.server
from pathlib import Path

import typer


def main(
    fixture: Path = typer.Argument(..., help="Path to the fixture .txt file."),
    port: int = typer.Option(8000, help="Port to listen on."),
) -> None:
    content = fixture.read_bytes()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/metrics":
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; version=0.0.4")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)
            else:
                self.send_response(404)
                self.end_headers()

        def log_message(self, fmt: str, *args: object) -> None:
            pass  # suppress request logs

    server = http.server.HTTPServer(("", port), Handler)
    typer.echo(f"Serving {fixture} on http://localhost:{port}/metrics")
    typer.echo(f"Run: vllm-doctor --url http://localhost:{port}/metrics --verbose")
    typer.echo("Press Ctrl+C to stop.")
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == "__main__":
    typer.run(main)
