# Porter Duty — Duplicate-Port Prevention

Standing rule (2026-09-29, user's order): **every porter runs the dup-check BEFORE writing any Rust code.** This is the 3rd-duplicate prevention process (feature-flags, batch-B, transport were all re-ports).

## The gate (run all four, in order)

### (a) Grep crates for the Swift type name AND the concept

Work from a clean `origin/next` worktree (never the branch you are about to write on):

```bash
git worktree add ~/workspace/wt-dupcheck origin/next
cd ~/workspace/wt-dupcheck
rg -il '<SwiftTypeName>|<concept>' crates/
```

If any Rust file already implements the type or concept → **STOP. Do not port.** Point the sidecar row at the existing Rust target instead.

Notes:
- Search the **type name** (`GitHeadReader`, `RemoteDirectTransport`) AND the **concept** (`git_head`, `direct_transport`, `plaintext`).
- Also search for the **function name** you plan to write (`pub fn stable_hash`) against existing `pub fn`s.
- Private `fn`s count too: if the logic exists privately in a crate, make it `pub` and reuse it — do not write a second copy.

### (b) Read the sidecar for the file's existing target

```bash
rg -l '<SwiftFileName>' docs/parity/swift-port/
```

Read the row(s) found. If status is already `partial` or `ported` → **STOP. Do not re-port.** Extend the existing row or its Rust target if behavior is missing.

### (c) Never re-port a file whose row is already partial/ported

`ported` means **EVERY behavior** of the Swift file is covered by Rust + tests. If your own notes list unported behavior, the row is `partial`, with explicit `dropped: <reason>` entries — never `ported`. `not portable` is not a status.

### (d) The integrator rejects any new module whose functions duplicate an existing `pub fn`

Before adding a new file, list its `pub` items and grep each against `crates/`:

```bash
rg -n "pub fn <name>" crates/
```

One function, one home. If a duplicate is found, delete your copy and re-point your tests at the single implementation.

## Duplicate patterns seen (do not repeat)

| # | What was duplicated | Existing on `next` | Fix |
|---|---|---|---|
| 1 | `feature_flags.rs` second copy | first copy already in `crates/` | one file, remove duplicate sidecar entry |
| 2 | `git_head_reader::current_branch` | `supercli-core/src/controller_host.rs:1796` `git_head_branch` (private, matches native `GitHeadReader`) | make core's `pub`, reuse |
| 3 | `stable_hash` (FNV-1a) | `supercli-core/src/worktrees.rs:39` `fnv1a` (private, same constants) | make core's `pub`, reuse |
| 4 | `remote_direct_transport.rs` (whole file) | `crates/supercli-client/src/direct_transport.rs` (`RemoteServerVersion`, `direct_transport_decision`, `bootstrap_deadline`, `PushTokenRegistrationRoute`) | `git revert`; point sidecar rows at existing files. **This copy also reintroduced the no-plaintext-fallback security fix — never re-port security-critical code without diffing against the current implementation.** |
| 5 | `remote_terminal_stream.rs` (whole file) | `crates/supercli-client/src/terminal_stream.rs` (`RemoteServerEndpoint`, `RemoteTerminalWebSocketCandidate`, `RemoteTerminalWsHello`, `web_socket_output_url`) | `git revert`; point sidecar rows at existing files |
| 6 | `host_binary()` resolver | `supercli-core/src/session_ops.rs:2196` `resolve_host_binary` (same `SUPERCLI_HOST_CMD` env var, same concept) | reconcile into one function |
| 7 | `MAX_INPUT_BYTES_PER_FRAME` 16KiB vs 32KiB | `crates/supercli-client/src/terminal_stream.rs:311` | one constant; verify the value against the Swift original |

## Before every porter dispatch

The orchestrator includes this checklist in every porter prompt, and a light **dup-check worker** runs steps (a)–(d) against the proposed file list before the porter writes code. The porter proceeds only on a clean dup-check report.
