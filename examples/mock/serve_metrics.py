"""Serve a mock metrics scenario over HTTP for local testing.

Usage:
    python3 examples/mock/serve_metrics.py examples/mock/metrics.txt
    python3 examples/mock/serve_metrics.py examples/mock/prometheus.json --port 9090

Then in another terminal:
    vllm-doctor diagnose http://localhost:8000 --verbose
"""

import argparse
import http.server
import json
from pathlib import Path
from urllib.parse import parse_qs, urlparse


def _prometheus_response(fixture, query):
    result = next((value for key, value in fixture.items() if key in query), [])
    body = {"status": "success", "data": {"resultType": "vector", "result": result}}
    return json.dumps(body).encode()


def main():
    parser = argparse.ArgumentParser(description="Serve a mock metrics scenario over HTTP.")
    parser.add_argument("fixture", type=Path, help="Path to the scenario .txt or .json file.")
    parser.add_argument("--port", type=int, default=8000, help="Port to listen on.")
    args = parser.parse_args()

    fixture = args.fixture
    port = args.port

    if fixture.suffix == ".json":
        prometheus_fixture = json.loads(fixture.read_text())
        scrape_content = None
    else:
        prometheus_fixture = None
        scrape_content = fixture.read_bytes()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
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

        def log_message(self, fmt, *args):
            pass

    server = http.server.HTTPServer(("", port), Handler)
    url = f"http://localhost:{port}" if prometheus_fixture is not None else f"http://localhost:{port}/metrics"
    print(f"Serving {fixture} on {url}")
    print(f"Run: vllm-doctor diagnose {url} --verbose")
    print("Press Ctrl+C to stop.")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopped.")
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
