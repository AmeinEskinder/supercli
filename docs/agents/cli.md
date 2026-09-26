<!-- Split out of the repo-root AGENTS.md (2026-08-31). The root AGENTS.md holds the map, hard rules, and invariants; this file is the full detail for its topic. -->

## Scriptable CLI

`crates/supercli-cli/src/cli.rs` is the one-shot command dispatcher for the
`supercli` binary. It is a frontend over shared Host/session contracts, not a
second state implementation. `claim_workspace_flag` runs before dispatch, so
every one-shot verb automatically targets the selected `SUPERCLI_HOME` when
prefixed with `supercli --workspace NAME`.

One-shot writes must use the sanctioned shared-state primitives:

- `supercli_core::app_state::edit` for `app-state.json`: exclusive flock, raw
  `serde_json::Value` read-modify-write, unknown-key preservation, atomic
  rename, and `state_bus::announce(Change::AppState)`;
- the matching `session_ops` helper for Session markers/order/lifecycle;
- `state_bus::flush()` before process exit. The main dispatcher already does
  this after every handled one-shot command; never bypass that exit path for a
  new mutating verb.

Never deserialize `app-state.json` into `AppState` and serialize it back from a
CLI mutation. That typed round trip would drop fields owned by a newer app.
Never recover from a present-but-corrupt file by seeding defaults. Missing is a
fresh workspace; unreadable or malformed is an error.

### Workspace settings

`crates/supercli-cli/src/settings_cli.rs` owns this grammar:

```text
supercli settings list [--json]
supercli settings get <key> [--json]
supercli settings set <key> <value> [--json]
```

It deliberately exposes an allowlist, not arbitrary JSON paths:

| CLI key | Stored JSON | Set values | Effective fallback |
| --- | --- | --- | --- |
| `experimental_features.sessions_mcp` | nested boolean | `true`, `false` | `true` |
| `experimental_features.browser_mcp` | nested boolean | `true`, `false` | `true` |
| `browser_default_access` | string | `on`, `ask`, `off` | absent `on`; malformed `off` |
| `mcp_nonchild_write_access` | string | `ask`, `allow`, `deny` | `ask` |
| `theme` | string | `system`, `light`, `dark` | `system` |

Browser access reads use `BrowserAccess::from_state_str` semantics and fail
closed on malformed/non-string state. A nested experimental edit creates the
object only when absent; it refuses to replace a present non-object and keeps
unknown sibling gates. Values are validated before acquiring the write path,
so an unknown key/value leaves the file byte-for-byte untouched.

Human `get` prints the normalized value. JSON `list` is a key/value object;
JSON `get` and `set` return `{"key": ..., "value": ...}`. These shapes and
the parsed `SettingsCommand` separation are the future seam for dispatching
the same grammar through `settings.workspace.set`; local disk semantics are
the current implementation. Do not invent a CLI-only remote settings
protocol.

`experimental_features.sessions_mcp` and `.browser_mcp` are launch gates.
Changing them affects Sessions started or restarted afterward, not the
capability set captured by a running Session. Keep that warning in `supercli
settings --help` and the website CLI page.

### Preset organization

`supercli presets` mutates the same raw `presets` array through
`app_state::edit`:

```text
supercli presets star|unstar <label|id>
supercli presets enable|disable <label|id>
supercli presets reorder <label|id> <position>
```

Selectors resolve an exact id first, then one exact label; duplicate labels
are rejected instead of changing an arbitrary row. Reorder positions are
1-based at the shell boundary and become a zero-based final array slot
internally, matching the Host protocol's `sortOrder` semantics. Array order is
the display order and the first preset for a CLI is that CLI's default. Star
is the stored `quick_launch` flag; enable is the compatibility `enabled` flag.
Every mutation is idempotent and preserves unknown fields on the row, sibling
rows, and the document.

The current native preset product remains present-or-deleted and normalizes
stored global rows to enabled; headless/Host consumers still honor the
compatibility flag. Do not silently remove the flag or reinterpret disable as
delete. Native convergence, if chosen, is a separate product change.

### Supercli Link enrollment

`crates/supercli-cli/src/link_cli.rs` owns `supercli link` — the scripted
headless spelling of the activation the app's Settings ▸ Remote offers
(originally also offered by the now-removed interactive TUI's Settings ▸
Remote):

```text
supercli link enroll <key> [--json]
supercli link status [--json]
supercli link deactivate
```

Hard rules:

- **One activation implementation.** The CLI composes the exact
  `supercli_core::license` request/commit primitives the app's Settings ▸
  Remote uses (`request_activation` → `commit_activation` →
  `request_relay_entitlement_for_key` →
  `commit_relay_entitlement_for_activation`), so the locked durable
  suppression record, activation-pending intermediate state, and
  authoritative-rejection semantics are shared byte-for-byte. Never fork a
  second activation path here.
- **Never a client-side gate on anything local.** Enrollment only adds Link
  (off-LAN relay) authority; local/LAN behavior must not consult it.
- Exit codes are script vocabulary: `0` success (for `status`: usable Link
  authority), `1` definitive failure (invalid key, rejection, suppressed),
  `2` transient/retryable. An `enroll` that exits `2` after "activation
  committed" left the durable `activation_pending` state; a retry or a
  running `supercli serve` finishes it.
- A running serve needs no restart: its driver's Link maintenance observes
  the fresh key/entitlement on its next tick.
- `status` is read-only — it must not mint a Host identity or touch any Link
  file. `enroll` binds to `relay_uplink::ensure_host_id()` like every other
  Link consumer.

Evidence: `tests/cases/link_enroll.py` (shared fixtures in
`tests/link_fixtures.py`, also used by `link_lifecycle.py`).

### Host service packaging (`supercli serve install`)

`crates/supercli-serve/src/service_install.rs` implements
`supercli serve install|uninstall|status`, rendered from the verbatim-usable
templates in `packaging/service/` (launchd LaunchAgent + systemd `--user`
units; the templates document the exact anchor lines install rewrites).
Scope follows the serve rule: no `SUPERCLI_HOME` → machine unit; a registered
workspace home (recovered by `workspaces::current_scope()` after
`--workspace` re-homed the process) → scoped `--workspace NAME serve` unit;
an unregistered home is refused. Constraints:

- **per-user only** (LaunchAgent / `systemctl --user`), never root — the
  service owns `~/.supercli`, the Keychain, and the per-user machine lease.
  Docs must carry the macOS auto-login and Linux `loginctl enable-linger`
  notes.
- `uninstall` stops the managed service and removes only the unit file;
  workspace data and running Session PTYs are untouched.
- `launchctl`/`systemctl` are resolved via `PATH` and the unit directories
  derive from `HOME`/`XDG_CONFIG_HOME`, so tests drive real flows through
  shims (`tests/cases/serve_install.py`) and never register a real service.
  `SUPERCLI_SERVICE_MANAGER=launchd|systemd` overrides platform detection for
  tests only.
- `status` reports the unit + service-manager view and the existing lease
  truth (`service::is_running` / `driver::is_running_at`), exit 0 only while
  the Host service is actually running.

### Browser engine (`supercli browser install`)

`crates/supercli-cli/src/browser_cli.rs` — the one Browser MCP verb; every
decision lives in `supercli_core::browser_engine` so the CLI, the worker's
start-time install, and the MCP server can never disagree:

```text
supercli browser install [--check] [--json]
```

Installs (or confirms) the pinned `agent-browser` under
`~/.supercli/browser/bin` after sha256 verification against
`protocol/browser-engine-v1.json`; `--check` only reports and never
downloads. Exit codes: 0 engine ready · 1 download/hash/unsupported failure ·
3 (`--check`) missing or stale · 4 engine ready but no Chrome/Chromium on
this Host (the text names the binaries looked for; Supercli never installs a
browser). `--json` prints `{state, version, path, error?, browser: {path |
null, error?}}`. Sample (macOS):

```text
engine:  agent-browser 0.34.0 — ready
path:    /Users/me/.supercli/browser/bin/agent-browser
browser: /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
```

### The CLI as an MCP peer (`supercli mcp` and friends)

Everything the unified `supercli` MCP server can do, the CLI can do, through
the same in-process dispatcher (`supercli_core::mcp_host::call_tool`): the
same caller identity (`SUPERCLI_SESSION_ID`, or process ancestry), the same
per-call grants from the Session manifest, and the same cooperative write
policy with its approval prompt. An agent that prefers shell verbs over an
MCP client loses nothing.

```text
supercli mcp                                   # the tools
supercli mcp <tool>                            # that tool's help (the MCP help text)
supercli mcp <tool> <action> [key=value ...] [--json '{...}']
supercli browser open <url> | snapshot | click <target> | fill <target> <text>
              | type <target> <text> | press <key> | get <what> [target]
              | screenshot [--full] [--annotate] | scroll <dir> | wait [ms=…|selector=…]
supercli artifacts publish <image>             # artifacts add_to_gallery
supercli current                               # sessions current: you + pane neighbors
supercli report <summary> [--status update|done|blocked] [--details TEXT]
supercli worktree create <name> [--branch B] [--base REF] [--project ID]
supercli agents <action> [key=value ...]       # occupants, transcripts, wait
supercli skills <action> [key=value ...]
supercli apps describe|search|context [key=value ...]
```

`key=value` values that parse as JSON (`true`, `42`, `["down","enter"]`,
`"quoted"`) are passed as JSON; anything else is a string. Exit 1 prints the
tool's own error text, exactly what an MCP client would see. Outside a hosted
Session the caller is unknown and most actions refuse, like the server.

**`supercli send` / `supercli keys` follow the write policy.** From a terminal
outside Supercli they write straight to the Session socket (the operator is
the user). From inside a hosted Session — an agent's subprocess — they are
the MCP `send_text` / `send_keys` actions: the first write to another
Session blocks on the user's approval (desktop or phone), an approved pair is
remembered per the app-wide policy, a denial writes nothing, and a Session
can never write into itself. Inside a Session `keys` takes key names one per
word (`down enter`, `ctrl+c`); outside it keeps its raw escape sequence.
PTY proof: `cli_write_policy`.

### Agent integrations (`supercli integrations`)

A preset launches its command in your login shell exactly as typed. What
makes a recognized agent report busy/idle/attention and reach the unified
`supercli` MCP server is its **integration**: lifecycle hooks plus the MCP
shim registered in that CLI's own global configuration. It is installed
once per Host, explicitly, and never as a side effect of a launch:

```text
supercli integrations [list] [--json]
supercli integrations install <runtime> [--project DIR] [--json]
supercli integrations install --all [--json]      # every installable runtime whose CLI is on PATH
```

`<runtime>` is the short name (`claude`, `codex`, `gemini`, …), the catalog
id, or the command. `list` shows `installed`, `installed (refreshing)` (the
Host will rewrite it for this build), `not installed`, or `detection only`
(Pi: nothing to install). Amp and GitHub Copilot read hooks per project;
`--project DIR` writes that project's file. A marker under
`~/.supercli/integrations/` records the installing build; the workspace
worker re-runs installed integrations' installers after an upgrade so hook
scripts and `~/.supercli/bin/supercli-mcp` keep pointing at the running binary.
The integration is per user, like the provider configs it edits: markers,
hook scripts, and the shim live under the **machine home**
(`app_paths::machine_home()` — the real `~/.supercli` whenever the active
`SUPERCLI_HOME` is a workspace listed in the machine's registry
`~/.supercli/profiles.json`; an unregistered `SUPERCLI_HOME`, i.e. a blank
instance or a test, is its own machine home), so installing from any local
workspace installs for all of them and every workspace reports the same
status, with no environment plumbing. A remote Host has its own machine
home.
The Host verb is `integrations.install` (`POST /mobile/integrations/install`,
`{"runtimeID": …}`), and bootstrap's `availableAgents` rows carry
`integrationInstallable`/`integrationInstalled`, plus the package's `integrationSummary` / `integrationManualCommand` and the Host's `mcpShimPath`, for Settings ▸ Agents.

### Supercli Apps (`supercli apps`)

Official App discovery and installation are Host-owned, so the same commands
work in the default workspace, an isolated local workspace, and a remote
Linux/SSH Host:

```text
supercli apps list [--json]
supercli apps install <app-id> [--check] [--yes] [--json]
supercli apps update [<app-id>] [--check] [--yes] [--json]   # reinstall Apps behind the registry version (--check: exit 3 = available)
supercli apps link <app-id> <executable>       # dev mode: symlink a local build into the slot
supercli apps unlink <app-id>                  # remove only such a link
supercli open <path> [--with <app-id>] [--media-type <type>] [--json]
supercli open git:working-tree [--with diffs] [--json]
supercli open <resource-id> --kind <resource-kind> [--with <app-id>] [--json]
supercli settings openers set <file:media-type|resource:kind> <editor|system|app:id>
```

The embedded `protocol/app-registry.json` is the allowlist. Managed copies
land under `~/.supercli/apps/bin`; installation selects the Host platform,
requires the release archive's `.sha256` sidecar, extracts only the declared
binary, and atomically replaces it under a flock. `--check` reports exit 3
when the App is absent and never downloads. Bootstrap publishes the catalog,
installed subset, and typed opener map to capability-aware Controllers.
Direct installs prompt on an interactive terminal. Noninteractive automation
must pass `--yes`; without it the CLI fails closed. `--check` never prompts or
downloads.

`supercli open` is the user-facing dispatcher. It normalizes the argument into
a typed resource, resolves a workspace preference or the registry's
`default_for`, offers to install a missing official App only on an interactive
terminal, and passes the resource as one shell-safe argument to the exact
resolved executable. Inside an Supercli Session it uses the shared `app_open`
operation to create/reuse a companion pane. Outside a Session it launches the
same command as a new hosted App Session. Noninteractive calls never install
implicitly; the error names the exact `supercli apps install` command.

Opener preferences and managed binaries are per workspace Host. This is what
makes the same flow work on isolated local workspaces and SSH/Linux Hosts: the
registry, file/resource, installation, and App process all live on the Host.
The Controller never copies or executes a remote App locally.

### Desktop-session service (`supercli serve install --graphical`)

Linux only. Writes the `graphical-session.target`-bound variant of the
user unit (`packaging/service/supercli-serve-graphical.service`) so the Host
runs inside the desktop session used by graphical tools; launchd refuses the
flag (the app owns the desktop daemon on macOS). `uninstall` and `status`
take no flag; `status` additionally prints `unit variant:`,
`graphical-session.target:` (`is-active`), and `desktop session:` — the
display plus session bus visible to the calling shell, or the missing
piece. Detail: `docs/agents/serve.md`.

### Gates

At minimum, CLI settings/preset changes run:

```sh
(cd crates && cargo test -p supercli-cli settings_cli::tests)
(cd crates && cargo test -p supercli-cli --test settings_command)
(cd crates && cargo test -p supercli-cli)
```

The real-process case proves JSON output, validation-before-write, nested and
top-level unknown-field preservation, preset flags/order, and that the state
bus notification is delivered before the process exits. Changes that touch
workspace selection must also run the full `crates/supercli-cli/tests/run.sh`
matrix because every command composes with `--workspace`.

### Scheduled sessions

`crates/supercli-cli/src/schedule_cli.rs` owns this grammar:

```text
supercli schedule add --id ID --session SID --interval SECS \
    --tool TOOL [--arg KEY=VALUE ...] [--tool TOOL ...] \
    [--max-duration SECS] [--max-steps N] [--max-output BYTES] [--max-retries N]
supercli schedule list [--json]
supercli schedule pause <id> | resume <id> | remove <id>
supercli schedule run-once <id> [--json]
supercli schedule daemon
```

The policy contract lives in `crates/supercli-core/src/scheduled.rs`
(`ScheduledRunner`, `Scheduler`, `AutonomousPolicy`) and is the definition
of record; this section is the operator-facing summary:

- Schedules are explicit operator opt-in. Nothing runs without a schedule;
  there is no default-on, no discovery, no inference.
- One named session per schedule; `add` refuses unknown sessions, and a
  session archived later fails closed at run time with an audited record.
- A scheduled run executes an explicit ordered list of connector tool calls
  with no human present — never an agentic prompt loop. `Ask` tools are
  denied immediately (`NoHumanPresent`); explicit `Deny` stays denied.
- Defaults: 30 min, 200 steps, 4 MiB output, 0 retries. Absolute ceilings:
  60 s minimum interval, 24 h max duration, 10 000 steps, 3 retries with
  5 s/10 s/20 s backoff. Retries apply to transient failures and timeouts
  only; a run killed by a resource cap or denied by policy is never retried.
- Single-flight per schedule: an overlapping trigger is skipped and audited,
  never queued. The slot is released by RAII on every exit path, including
  panic unwind.
- Every trigger — success, failure, timeout, kill, overlap skip, invalid or
  paused spec — appends exactly one record to
  `<session-dir>/scheduled-runs.jsonl`. The registry
  (`<SUPERCLI_HOME>/schedules.json`) is written under an exclusive flock with
  atomic rename; a corrupt registry is an error, never an empty list.
- `daemon` is the only supported driver: it re-reads the registry every
  tick, fires due triggers, and notifies exactly once per trigger on
  persistent failure (after retries) through the existing notification path
  plus a stderr event for headless operation. Do not drive schedules from
  system cron — single-flight is enforced in-process, so a second driver
  would break the no-overlap guarantee.
- `remove` deletes the schedule but keeps the audit trail in the session
  dir. `run-once` fires one trigger through the same runner (audited
  identically); exit code 0 only on `Completed`.

### Durable runs

Scheduled triggers are journaled in a write-ahead runs database so a
`kill -9` of the daemon mid-run never loses or duplicates work
(`crates/supercli-core/src/durable_runs.rs`, `track-b-durable-runs`).

- **Write-ahead intent protocol.** Each step is journaled with
  `begin_step()` *before* its side effect runs and closed with
  `complete_step()` after. A crash between the two leaves an orphaned
  intent, never an ambiguous replay. The old `append_step()` (which could
  leave legacy NULL outcomes) is not used by the scheduler.
- **Per-kind reconciliation.** On restart, orphaned intents are classified
  by `StepKind`: `Model` and `Read` rerun safely; `FileWrite` is
  probe-completed by content hash; `IdempotentHttp` reruns idempotently;
  `OpaqueWrite` (unverifiable side effects) goes to `NEEDS_REVIEW` —
  fail closed, never replayed blind.
- **Proof.** 50-iteration real-`SIGKILL` chaos against external
  side-effect ground truth (not journal assertions): 50/50 terminal,
  0 duplicates, 36 `DONE` / 14 `NEEDS_REVIEW`, and only opaque writes
  needed review.
- **Fail closed by default.** `ScheduledRunner` refuses to fire when the
  runs DB is unavailable (`durable_required`, the default). The explicit
  opt-out is `--no-durable` on `run-once` / `daemon` — unjournaled mode is
  never the default.
- **Kill/resume.** A dedicated integration test `SIGKILL`s a real daemon
  process mid-run and restarts it: the same run id resumes, completed steps
  are not re-executed, each side effect appears exactly once.

### Operator memory (`supercli memory`)

A small durable key/value store for operator preferences and session facts
(`crates/supercli-core/src/memory.rs`, `crates/supercli-cli/src/memory_cli.rs`):

```text
supercli memory set <key> <value...> [--session <id>] [--longterm]
supercli memory get <key>
supercli memory promote <key>      # session scope -> longterm scope
supercli memory forget <key>
supercli memory list [--json]
```

- **Scopes.** `Session` entries belong to one session id; `Longterm`
  entries are operator-wide. `promote` moves a key from session to
  longterm explicitly — nothing is promoted automatically.
- **Atomic saves.** The store is written temp + fsync + rename under a
  lock; concurrent writers cannot tear it.
- **Operator profile.** Alongside memory, an operator profile
  (`crates/supercli-core/src/profile.rs`) keeps preferences and
  approval/denial counters fed by real human review decisions
  (`record_review` counts Human actors only). `approval_hint()` surfaces
  "usually approved/denied" signals to UIs — it is **advisory only**: a
  suggestion never grants anything by itself. Any auto-allow requires an
  explicit user action that creates a normal audited grant.
