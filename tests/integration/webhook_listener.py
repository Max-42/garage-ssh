#!/usr/bin/env python3
import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer


PORT = int(os.environ.get("PORT", "18080"))
STATE = {
    "count": 0,
    "events": [],
}


class Handler(BaseHTTPRequestHandler):
    def _send_json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/count":
            self._send_json(200, {"count": STATE["count"]})
            return

        if self.path == "/events":
            self._send_json(200, {"events": STATE["events"]})
            return

        self._send_json(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8", errors="replace")

        STATE["count"] += 1
        STATE["events"].append(
            {
                "path": self.path,
                "body": raw,
                "content_type": self.headers.get("Content-Type", ""),
            }
        )

        print(f"WEBHOOK_EVENT path={self.path} count={STATE['count']} body={raw}", flush=True)
        self._send_json(200, {"ok": True})

    def log_message(self, fmt, *args):
        return


if __name__ == "__main__":
    server = HTTPServer(("0.0.0.0", PORT), Handler)
    print(f"LISTENING {PORT}", flush=True)
    server.serve_forever()
