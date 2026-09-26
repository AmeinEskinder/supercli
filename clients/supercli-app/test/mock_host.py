#!/usr/bin/env python3
"""Mock supercli Host for e2e testing the Dart app.

Implements the minimal mobile gateway API:
- GET /mobile/sessions
- GET /mobile/approvals
- POST /mobile/approvals/answer
- GET /mobile/events/poll
"""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

# In-memory state
approvals = [
    {
        "id": "e2e-approval-1",
        "tool": "write_file",
        "summary": "Write /tmp/e2e-proof.txt",
        "detail": "Test approval for e2e verification",
        "generation": 1,
    }
]
answered = []

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass  # Quiet

    def _send(self, data, code=200):
        body = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/mobile/sessions"):
            self._send({"sessions": [
                {"id": "sess-1", "title": "E2E Test Session",
                 "updated_at": "2026-09-26T00:00:00Z", "unread_count": 0}
            ]})
        elif self.path.startswith("/mobile/approvals"):
            pending = [a for a in approvals if a["id"] not in answered]
            self._send({"approvals": pending})
        elif self.path.startswith("/mobile/events/poll"):
            self._send({"type": "timeout"})
        else:
            self._send({"error": "not found"}, 404)

    def do_POST(self):
        if self.path.startswith("/mobile/approvals/answer"):
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            aid = body.get("id")
            approved = body.get("approved", False)
            if aid and aid not in answered:
                answered.append(aid)
                print(f"MOCK HOST: approval {aid} answered (approved={approved})", flush=True)
            self._send({"ok": True, "id": aid})
        else:
            self._send({"error": "not found"}, 404)

if __name__ == "__main__":
    server = HTTPServer(("127.0.0.1", 8137), Handler)
    print("Mock Host listening on 127.0.0.1:8137", flush=True)
    server.serve_forever()
