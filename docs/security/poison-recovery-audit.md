# Poison-recovery audit: `into_inner` conversions (Phase 11 P6)

Phase 10 R2 converted 27 production `.lock().unwrap()` sites to
`.lock().unwrap_or_else(|e| e.into_inner())` so a panicked holder no longer
takes the Host down. Phase 11 added more sites in the same pattern. This
table audits every production `into_inner` recovery site in
`session_host.rs`, `session_io.rs`, and `rate_limit.rs` for half-updated
guarded state.

Method: for each site, identify the mutex-protected invariant, determine
whether the critical section contains a realistic panic path between
mutations, and assess what a recovered half-updated state would do.

## Verdict

No site has a realistic panic path that leaves safety-critical state
half-updated. The conversions are sound. Two sites have theoretical
half-update windows that require OOM (or equivalent unrecoverable
failure); both degrade gracefully and are noted below.

## Table

| # | Site | Guarded invariant | Panic between mutations? | Assessment |
|---|------|-------------------|--------------------------|------------|
| 1 | session_host.rs:731 `title_buffer` | Prompt-extraction scratch buffer | `extract_submitted_prompt` takes `&mut`; a panic mid-extract leaves a half-consumed buffer | Cosmetic only: wrong window title, self-heals on next prompt. Safe. |
| 2 | session_host.rs:1196 `runtime` | `HostRuntime` (child, PTY, shell) | Read-only (`shell_executable`, `require_fresh_owned_shell` check) | Safe. |
| 3 | session_host.rs:1228 `runtime` | `HostRuntime` | Read-only check | Safe. |
| 4 | session_host.rs:1237 `broadcaster` | Output broadcaster offsets | Read-only (`next_offset`) | Safe. |
| 5 | session_host.rs:1241 `runtime` + `broadcaster` | `HostRuntime`, broadcaster | Read-only | Safe. |
| 6 | session_host.rs:3482 `manifest_health_cache` | `HashMap<session_id, health>` | `retain` + `insert`; `HashMap` ops are panic-safe for the map structure | Worst case: stale/missing cache entry, re-queried. Safe. |
| 7 | session_host.rs:3502 `manifest_health_cache` | Same | Single `remove` | Safe. |
| 8 | session_host.rs:3936 `UPDATE_LOCK` | None (`Mutex<()>` pure exclusion) | No data to corrupt | Safe by construction. |
| 9 | session_host.rs:5348 `cancellation_viewport` | Terminal viewport (read-model) | Read-only (`current_screen_text`) | Safe. |
| 10 | session_host.rs:5403 `runtime_for_observer` | `last_runtime_observation` | Single field write (`= None`) | Atomic w.r.t. panics. Safe. |
| 11 | session_host.rs:5409 `runtime_for_observer` | Same | Single field write + reads | Safe. |
| 12 | session_host.rs:5504 `runtime_for_observer` | Same | Single field write | Safe. |
| 13 | session_host.rs:5602 `viewport_for_menu` | Terminal viewport (read-model) | Reads (`current_screen_text`, `terminal_mode_state`) | Safe. |
| 14 | session_io.rs:505 `viewport` | Terminal viewport | Read-only (`cursor_position`) | Safe. |
| 15 | session_io.rs:507 `runtime` | `HostRuntime` | `writer.write_all` (returns `Result`, `let _ =`) | Safe. |
| 16 | session_io.rs:556 `viewport` | Terminal viewport | `feed(&chunk)` mutates the parser; a panic mid-parse leaves a half-parsed frame | Read-model derived from the byte stream; garbled frame self-heals on next chunk. Not an approval gate. Safe. |
| 17 | session_io.rs:562 `viewport` + `broadcaster` | Viewport, broadcaster | `feed` + `broadcast_chunk`; `broadcast_chunk` updates `next_offset` then pushes — OOM between them is the only panic path | Unrecoverable anyway. Safe. |
| 18 | session_io.rs:620 `agent_restart_lock` | None (`Mutex<()>` pure exclusion) | No data | Safe by construction. |
| 19 | session_io.rs:621 `runtime` | `HostRuntime` | Write loop with `Result` handling | Safe. |
| 20 | session_io.rs:630 `viewport` | Terminal viewport | Read-only | Safe. |
| 21 | session_io.rs:1113 `broadcaster` | Broadcaster | `mark_exited` sets flag, retains/clears subscribers; no panicking ops | Safe. |
| 22 | session_io.rs:1206 `runtime` | `HostRuntime` | Read-only (`try_wait`) | Safe. |
| 23 | session_io.rs:1318 `viewport` | Terminal viewport | Read-only snapshot | Safe. |
| 24 | session_io.rs:1353 `agent_restart_lock` | None (`Mutex<()>` pure exclusion) | No data | Safe by construction. |
| 25 | session_io.rs:1354 `runtime` | **Idempotency invariant:** `recent_write_ids` must record exactly the writes delivered to the PTY | **Theoretical:** panic between the PTY write loop and `record_applied` leaves the id unrecorded; a retry would duplicate partial input | No realistic panic path in the critical section (`write` returns `Result`; `feed`/`record_applied` panic only on OOM). The design already tolerates re-delivery on `Err`. Not made worse by `into_inner`. See note below. |
| 26 | session_io.rs:1364 `viewport` | Terminal viewport | Read-only | Safe. |
| 27 | session_io.rs:1417 `runtime` | `pty_cols`/`pty_rows` mirror the kernel PTY size | **Theoretical:** panic between the kernel resize and the field updates leaves fields stale | The dedup check (`pty_cols == cols && pty_rows == rows`) makes this self-healing: a redundant resize, never wrong behavior. Safe. |
| 28 | session_io.rs:1441 `viewport` | Terminal viewport | `resize` reflows (read-model) | Safe. |
| 29 | session_io.rs:1471 `agent_restart_lock` | None (`Mutex<()>` via `TryLockError::Poisoned`) | No data | Safe by construction. |
| 30 | session_io.rs:1504 `viewport` | Terminal viewport | Read-only snapshot | Safe. |
| 31 | session_io.rs:1536 `agent_restart_lock` | None (`Mutex<()>` pure exclusion) | No data | Safe by construction. |
| 32 | session_io.rs:1537 `runtime` | `HostRuntime` | `terminate_hosted_runtime`: signals + `kill`, no field mutations | Safe. |
| 33 | session_io.rs:1653 `viewport` | Terminal viewport | Read-only snapshot | Safe. |
| 34 | session_io.rs:1660 `viewport` + `broadcaster` | Viewport, broadcaster | Read-only | Safe. |
| 35 | session_io.rs:1676 `runtime` | `HostRuntime` | Read-only field copies | Safe. |
| 36 | session_io.rs:1760 `runtime` | `HostRuntime` | Read-only (`hook_input.clone`) | Safe. |
| 37 | session_io.rs:1951 `exit` | `Option<ExitStatus>` | Read-only clone | Safe. |
| 38 | session_io.rs:2194 `slot` | `Option<ExitStatus>` | Single assignment (`= Some(status)`) | Atomic w.r.t. panics. Safe. |
| 39 | rate_limit.rs:97 `buckets` | `HashMap<(device, endpoint), Bucket>` | `entry().or_insert_with()` is panic-safe; `try_consume` is plain arithmetic | Worst case: token count off by one (fail open/closed by a single request). Not safety-critical. Safe. |

## Note on site 25 (write idempotency)

The check → PTY write → `record_applied` sequence is the one place where a
panic could break an exactly-once invariant: the PTY is an external side
effect, so "compute first, commit after" cannot make the write itself
atomic. The mitigation is that the critical section contains no realistic
panic path — every fallible operation returns `Result` — and the
pre-existing design already treats a failed write as retryable. The
`into_inner` conversion preserves this behavior; it does not introduce a
new half-update risk. No code change required.

## Note on viewport sites (9, 13, 14, 16, 20, 23, 26, 28, 30, 33, 34)

The terminal viewport is a read-model derived from the PTY byte stream. A
half-parsed viewport after a recovered panic produces at worst a garbled
frame or a missed menu-detection tick, both self-healing on the next
chunk. The viewport is never an approval gate. No code change required.
