# supercli-host

The standalone host binary built on `supercli-core`. Clients (the Mac app, the
TUI, `supercli-attach`) spawn it in different argv modes rather than linking a
daemon:

- `__session_host__` — host one PTY session (the process that owns the
  terminal and survives app restarts)
- `__mcp__` — the `supercli` MCP server a session's agent talks to
  (sessions/browser/computer domains)
- `__remote__` — the TLS/WSS remote-control server for paired controllers
  (phones, other Macs) + relay uplink
- `__remote_attach__` — stdio bridge into another Supercli's `__remote__`
  server (gated by `SUPERCLI_REMOTE_ATTACH=1`)
- `__transcript__` — provider transcript reads (`snapshot`, `stream`,
  `history`, `markdown`)
- `__viewport__` — parsed-screen snapshots of a hosted session

Distributed two ways: bundled inside Supercli.app, and as part of the CLI
install (`curl -fsSL https://supercli.com/install.sh | sh`) alongside the
`supercli` TUI.
