#!/usr/bin/env python3
"""Bridge: serve /mobile/bootstrap from the LIVE supercli Host.

Reads real sessions via `supercli ls --json` (talks to the live Host over
its Unix socket) and serves them in the /mobile/bootstrap wire format.
This is real Host data, not fixtures — the sessions come from the running
`supercli serve`.
"""
import json
import subprocess
import os
from http.server import HTTPServer, BaseHTTPRequestHandler

SUPERCLI_HOME = os.path.expanduser('~/workspace/wt-appshell/.supercli-test')
SUPERCLI_BIN = os.path.expanduser('~/workspace/wt-appshell/crates/target/debug/supercli')

def real_sessions():
    """Get real sessions from the live Host."""
    env = dict(os.environ, SUPERCLI_HOME=SUPERCLI_HOME)
    try:
        out = subprocess.run(
            [SUPERCLI_BIN, 'ls', '--json'],
            capture_output=True, text=True, env=env, timeout=10,
        )
        sessions = json.loads(out.stdout) if out.stdout.strip() else []
        # Map to the bootstrap wire format.
        return [
            {
                'id': s.get('id', ''),
                'title': s.get('title') or s.get('id', 'untitled'),
                'updatedAt': s.get('updated_at') or s.get('updatedAt') or 0,
                'unread_count': s.get('unread_count', 0),
            }
            for s in sessions
        ]
    except Exception as e:
        print(f'bridge: failed to get sessions: {e}', flush=True)
        return []

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/mobile/bootstrap':
            sessions = real_sessions()
            body = json.dumps({
                'sessions': sessions,
                'pendingApprovals': [],
            }).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            print(f'bridge: served bootstrap with {len(sessions)} real sessions', flush=True)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, *args):
        pass

if __name__ == '__main__':
    port = int(os.environ.get('BRIDGE_PORT', '8137'))
    server = HTTPServer(('127.0.0.1', port), Handler)
    print(f'bridge: serving real Host bootstrap on 127.0.0.1:{port}', flush=True)
    server.serve_forever()
