# Doctor and troubleshooting

`unpeel doctor` checks the health of your workspace:

```sh
unpeel doctor
unpeel doctor --json   # machine-readable
```

## What doctor checks

- **Home-directory permissions** — `~/.unpeel` must be owner-only (0700).
  Group/world-readable permissions fail the check.
- **Review-chain integrity** — every session's `action-reviews.jsonl` is
  verified against its hash chain. A single flipped byte fails verification.
- **Stale leases** — schedule leases whose owner is gone are reported.
- **Clock skew** — the system clock is compared against review timestamps;
  large skew warns because lease expiry and review ordering depend on it.

## Troubleshooting

**Permissions check fails:**
```sh
chmod 700 ~/.unpeel
unpeel doctor
```

**Chain verification fails:** do not delete the log. The failure pinpoints
the session and the first bad line. Restore from a
[backup](backup.md) taken before the corruption, or export the session and
start fresh — the tamper-evidence is working as designed.

**Stale leases:** a stale lease means its owner crashed or was fenced.
`unpeel doctor` reports them; they expire on their own and are never
resurrected by migration.

**Host refuses to start (invalid config):** run `unpeel config check` to
see the offending setting with its path and reason, fix the value in
`app-state.json`, and restart. See the [config reference](config-reference.md).
