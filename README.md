# SuperCLI

SuperCLI is an **agent-first terminal multiplexer**, written in Rust. Sessions
keep running on your own machines, know when the agent inside them needs you,
and give that agent a browser and its sibling sessions to work
with. This repository is the whole product apart from the website and the
operated Link service: the server, the Mac app, and the iPhone/iPad app,
built from one tree at one version.

**Why this multiplexer**

- 🔁 **Sessions outlive everything.** Close the window, quit the app, drop the
  SSH connection, reboot the client, upgrade SuperCLI: the agent keeps working.
  One shared PTY core per workspace runs every session as an event-driven
  task, and a new build takes over running terminals in place, no restart.
- 🚦 **It knows what the agent is doing.** Provider hooks, runtime detection,
  and a terminal-viewport scanner turn "some process is printing" into busy,
  idle, and *needs you*, per session, with notifications to whichever client
  you are holding. Resume re-runs the agent with its own conversation id
  after a crash or a Host upgrade.
- 🧰 **Built-in MCP tools.** Every session has the single `supercli` MCP server:
  an isolated real browser with screenshots as reviewable artifacts,
  presets and worktrees, and the session gallery. The browser engine is
  Host-installed and pinned; nothing to set
  up per agent.
- 💬 **Agents can talk to each other.** From inside a session an agent can
  list its siblings, read their screens and transcripts, wait for one to go
  idle, and send text to another. Reads are open; the first write into
  another session asks you, and approved pairs are remembered. Sessions are
  created and closed by people, never by agents.
- 📲 **Drive real devices.** Android emulators stream over native scrcpy
  (pinned server, no `scrcpy` CLI needed) and iOS simulators over baguette;
  touch coordinates use device points end-to-end. The web Devices panel and
  `/farm` wall show live screens with tap/type/swipe, and `scripts/device-bench.sh`
  reproduces the same `metrics.json` on a Mac with a GPU. Detail:
  [`docs/device.md`](docs/device.md).
- 🪝 **Script it with hooks.** Frappe-style `doc_events` on `ToolCall`
  (`before_execute` runs after the write-ahead review is fsynced, before any
  tool bytes), `Approval` (`before_submit` / `on_submit` / `on_cancel`),
  and more. Handlers are shell commands or localhost webhooks in
  `~/.supercli/hooks.toml`, ordered by priority, time-boxed, tighten-only
  (escalate turns Allow into Ask — it re-enters the approval flow, never
  fails closed), with a durable outbox, dead-letter file, and
  `supercli hooks list|test|trace`. Detail: [`docs/events.md`](docs/events.md).
- ⏰ **Scheduled runs survive `kill -9`.** Every scheduled trigger is
  journaled write-ahead: `begin_step` before the side effect,
  `complete_step` after. A crashed daemon reconciles on restart — rerunning
  what is safe, hash-probing file writes, failing closed on opaque writes —
  with zero duplicates. The daemon refuses to fire unjournaled unless you
  pass `--no-durable`. Detail: [`docs/agents/cli.md`](docs/agents/cli.md#durable-runs).
- 🧠 **Operator memory.** `supercli memory set|get|promote|forget|list` with
  session and longterm scopes, atomic saves, and an operator profile whose
  approval hints are advisory only — suggestions never grant anything.
  Detail: [`docs/agents/cli.md`](docs/agents/cli.md#operator-memory-supercli-memory).
- 🧑‍💻 **Any agent, any task.** Claude Code, Codex, Gemini, Cursor Agent, Grok,
  Kimi, Kiro, Cline, Amp, OpenCode, Muse Code, Antigravity, Pi, or anything that runs in a
  terminal, for coding, research, writing, ops, or design. It is a terminal,
  not a code editor: you follow an agent through its terminal, its transcript,
  and the screenshots it takes, the same way whether it is fixing a bug or
  booking a trip.
- 📱 **Steer it from anywhere, on hardware you own.** Pair a Mac or a phone
  with a one-time code. On your own network or VPN the connection is direct;
  away from home it goes through SuperCLI Link, an end-to-end encrypted relay
  that only ever sees ciphertext. Your sessions, transcripts, and screenshots
  never live on a server you don't control.
- 🗂️ **Plain files, one protocol.** Every session is a directory under
  `~/.supercli/`: a manifest, a bounded output journal, a control socket. You
  can read, back up, or script it with ordinary tools. Every client speaks the
  same versioned protocol to every server, so a Mac hosting sessions and a
  Linux box hosting sessions look identical from the phone.
- 🪶 **Lean.** A few MiB for a Host with dozens of sessions; per empty session
  about 0.1 MiB, per 10k-line session about 0.4 MiB, measured and tracked in
  CI (`scripts/bench-memory.sh`).

**Clients**

- Mac app: [supercli.com/download/mac](https://supercli.com/download/mac)
- iPhone / iPad: [supercli.com/ios](https://supercli.com/ios)
- Docs, including headless hosting: [supercli.com/docs](https://supercli.com/docs)

This repository holds the Host service, the shared PTY core, the unified
`supercli` MCP server, the built-in agent runtimes, the CLI, and the Host
protocol — and the Mac and iOS app sources under
[`clients/`](clients/) (`clients/native`, `clients/ios`, `clients/shared`) plus the C-ABI
bridge crate the Mac app links, so app and server always build from one tree.
How to build and test them: the "Apple clients" section of
[`AGENTS.md`](AGENTS.md#apple-clients).

## Install

Mac or Linux:

```bash
curl -fsSL https://supercli.com/install.sh | sh
```

That installs `supercli`, `supercli-host`, and `supercli-attach` (Apple silicon and
Intel Macs, Linux x86_64 and aarch64; Ubuntu 20.04 / Debian 11 or newer). The
installer verifies the archive against its SHA-256 sidecar and refuses to
install otherwise.

## Quickstart

**1. Start the Host and pair your phone.**

```bash
supercli serve            # the Host service (leave it running)
supercli pair             # shows a one-time code / QR — scan it in the iPhone app
```

On your own network the phone connects directly; away from home it goes
through SuperCLI Link, the end-to-end encrypted relay. `supercli serve install`
registers the per-user boot service (launchd / systemd) so the Host survives
reboots.

**2. Open your first session.**

```bash
supercli new --command "claude" --cwd ~/project
supercli ls               # sessions, status, project, command
```

The agent runs inside a hosted terminal on your machine. Close the window,
quit the app, drop the connection — the session keeps running.

**3. Approvals.** When the agent tries something gated — using the browser,
writing into another session — it blocks and your phone (or Mac app) shows
an approval prompt. Approve or deny; an approved pair is remembered per the
app-wide policy, a denial writes nothing. Sessions are created and closed by
people, never by agents.

**4. Cancel a running turn.** If the agent is off the rails, hit Stop in the
app (or the phone): the Host interrupts the running turn and marks anything
in flight as ambiguous — never silently failed, never auto-retried — so you
can see exactly what may or may not have happened. From a terminal attached
to the session, Ctrl-C does the same where the app's Stop is unavailable.

The full CLI: `supercli help`.

## Run a Host

```bash
supercli serve            # the Host service: every registered workspace, one worker each
supercli serve install    # per-user boot service (launchd on macOS, systemd --user on Linux)
supercli pair             # show a one-time code / QR for a Controller (Mac app, iPhone)
supercli new --command "claude" --cwd ~/project
supercli ls               # sessions, status, project, command
```

`supercli serve` owns the machine lease, supervises one worker per workspace,
ingests provider hooks, answers Controllers over the local socket, LAN
(Direct), and SuperCLI Link, and keeps every session alive across upgrades.
`supercli --workspace NAME serve` runs a single isolated workspace (the container
spelling). The full CLI: `supercli help`.

The one-shot verbs, `supercli settings`, `supercli presets`, and the test gates
they share with the clients are documented in
[`docs/agents/cli.md`](docs/agents/cli.md).

## How sessions survive

- **Shared PTY core.** The worker starts one detached `supercli-host
  __pty_core__` per workspace and every session runs inside it as an
  event-driven task, not as a process of its own. Per empty session it costs
  0.12 MiB, per filled 10k-line session 0.39 MiB, and each attached client
  about 1 KiB on the server side (measured on macOS 26; the recipe is
  `scripts/bench-memory.sh`; the targets are tracked in the private design
  records).
- **Journal + sockets.** Each session lives under
  `~/.supercli/app-sessions/<id>/`: `manifest.json` (identity, state, pid with
  start-time identity so a recycled pid is never signalled), `output.bin` (a
  logically append-only journal with monotonic lifetime offsets and a bounded
  retained tail), and `session.sock` (write / resize / ping / kill, and an
  exact VT snapshot for attaching clients from the Host's resident
  libghostty-vt grid).
- **In-place core upgrade.** A newer core takes over a running one over
  `SCM_RIGHTS` (`__pty_core__ --takeover`, triggered by the service on build
  skew), so upgrading SuperCLI never restarts a terminal. Sessions never depend
  on the worker: the service can stop and restart while every PTY keeps
  running.
- **Clients are attachments.** `supercli-attach <id>` replays the journal tail
  (or the snapshot) and then bridges stdio to `session.sock`; the Mac app
  runs it inside its Ghostty surfaces, and remote Controllers stream the same
  journal over the Host protocol. Any client can restart without touching the
  agent.

Detail: [`docs/agents/pty-core.md`](docs/agents/pty-core.md),
[`docs/agents/serve.md`](docs/agents/serve.md),
[`docs/agents/session-model.md`](docs/agents/session-model.md).

## Host protocol

One protocol for every Controller and every Host. A Controller never cares
whether it is talking to a Mac app Host or a headless Linux box; SSH, LAN
(Direct), and SuperCLI Link are transports for the same contract, never second
sets of verbs. Capabilities are advertised, not guessed: bootstrap carries a
major-versioned, additive `hostProtocol` descriptor whose stable operation
ids come from [`protocol/host-capabilities-v1.json`](protocol/host-capabilities-v1.json),
and every Host implementation runs the same
[`protocol/host-conformance-v1.json`](protocol/host-conformance-v1.json)
cases. The `protocol/` directory (capability ledger, conformance fixtures,
pane-layout operations, direct-path rules, relay KAT vectors, the App
registry) ships verbatim inside every CLI archive so a pinned client can read
the contracts it was built against. A protocol change is a public pull
request here plus a version bump on the client side.

## Runtimes

Provider knowledge lives in one package per agent under
[`runtimes/<slug>/`](runtimes/): a `runtime.toml` descriptor, a Rust adapter
(the integration installer, resume identity, transcript discovery), hook
assets, and fixtures. Launching is provider-neutral: a preset runs its command
in your login shell exactly as typed. Each agent's hooks and MCP registration
are its *integration*, installed once per Host with
`supercli integrations install <runtime>` (or Install integration on Settings ▸ Agents)
into that CLI's own configuration, and kept current by the Host after
upgrades. The build discovers the packages and generates the registry, so
adding an agent never touches a central list. Contribution contract:
[`runtimes/README.md`](runtimes/README.md); per-provider notes:
[`docs/agents/providers.md`](docs/agents/providers.md).

Busy / idle / needs-attention state comes from real provider hook
integrations, never from guessing at output; select menus drawn by agents are
detected from the parsed viewport.

## The `supercli` MCP server

`supercli-host __mcp__` is one MCP server every capable session gets: `sessions`
(inspect, read the screen, wait for text, send input to sibling sessions under
an approval-controlled write policy), `agents` (occurrence-bound runtime
occupants and their transcripts), `workspace` (presets, git worktrees),
`artifacts` (screenshots as reviewable gallery items), `browser` (a real,
isolated browser per session driven over CDP), `apps`, and `skills`. Reads are
open; every write to another session follows the user's policy and asks by
default; session creation stays user-only. Detail:
[`docs/agents/sessions-mcp.md`](docs/agents/sessions-mcp.md),
[`docs/agents/browser-mcp.md`](docs/agents/browser-mcp.md).

## Clients

- **Mac app** — [`clients/native`](clients/native). The desktop client: a Controller
  of the bundled Host service plus the platform adapter (notifications,
  Keychain, approvals). Signed,
  notarized builds: [supercli.com/download](https://supercli.com/download).
- **iPhone / iPad app** — [`clients/ios`](clients/ios). A remote Controller: steer
  every session from your phone over your network or through SuperCLI Link.
  Builds ship through TestFlight; see [supercli.com](https://supercli.com).
- **Shared Swift package** — [`clients/shared/SupercliShared`](clients/shared/SupercliShared):
  pairing, the Host protocol client, and the end-to-end relay crypto both apps
  use, pinned to the same test vectors as the Rust side.
- **gpuidart desktop app** — `clients/supercli-app` (on the feature branch
  `track-b-app`; integrates with `supercli-next`). The cross-platform client
  on [gpuidart](https://github.com/ameineskinder/gpuidart): retained-mode
  GPU-accelerated UI in Dart, talking to a real Host over TLS with Bearer
  auth and blocking approvals. Keyboard-first via the `UiAction` API
  (`Ctrl+Enter` approve, `Ctrl+Shift+Enter` deny, `Ctrl+L` focus composer).
  Proof: `dart analyze` 0 issues, `dart test` 19/19, and a headless
  Host-connection e2e where the Dart app answers a real approval and the
  Host asserts `approved == true` and `answered_by == Some("paired-device")`
  (`dart test` + the Rust e2e both run in `apps.yml`). The same approval
  driven through the rendered window's `Ctrl+Enter` path is the remaining
  proof item.
- **`supercli` CLI** — this repository's `crates/supercli-cli`, for terminals and
  headless Hosts.

Every client speaks the Host protocol in [`protocol/`](protocol/); a headless
Linux Host is driven from the Mac app or the phone exactly like a Mac Host:
[supercli.com/docs/headless-host](https://supercli.com/docs/headless-host).
Building the apps from source is covered in the "Apple clients" section of
[`AGENTS.md`](AGENTS.md).

## Open source boundary

The server — this repository — is public under the MIT license
(`LICENSE`). The Mac and iOS app sources are here too
(`apps/`); official signed builds are published only by the SuperCLI team.
The only closed component is
the backend of the operated SuperCLI Link service (accounts, seats,
entitlements, rendezvous, relay, push): everything local and direct is free
and has no Link dependency. Design records and plans live in a private
archive repository; this repository documents what ships.

## Development

Rust 1.88 or newer. No Node runtime is required to run SuperCLI; Bun is only
used by the release scripts.

```bash
cargo build --manifest-path crates/Cargo.toml -p supercli-cli -p supercli-host
cargo build --manifest-path crates/supercli-attach/Cargo.toml   # standalone crate
cargo build --release --manifest-path crates/apps/Cargo.toml   # first-party Apps + App Kit (own workspace)
bun run apps:link                                              # dev mode: link those builds into ~/.supercli/apps/bin
SUPERCLI_HOME=/tmp/supercli-dev crates/target/debug/supercli serve  # isolated state
cargo test --manifest-path crates/Cargo.toml --workspace
crates/supercli-cli/tests/run.sh          # the real-PTY case matrix (~8 min)
scripts/verify-attach.sh                # attach replay / echo / snapshot smoke
```

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first; [`AGENTS.md`](AGENTS.md) is
the map of how the session system fits together and what must stay aligned.

## Releases

Server releases are CLI archives per channel on Cloudflare R2 behind
supercli.com (`bun run release:cli`); the installer above reads the same
bucket. Every archive carries `BUILD_PROVENANCE.json`,
`THIRD_PARTY_NOTICES.txt`, and `protocol/`. Details:
[`docs/agents/releases.md`](docs/agents/releases.md). Third-party licenses
for what this repository vendors or fetches at runtime: [`NOTICE.md`](NOTICE.md).
