# `unpeel self-update --check` — Design (Phase 14, item 4)

## Scope for this phase

- Design the check/update flow. **No network calls in this phase.**
- Local-file dry run only: `--check` reads a local manifest file.
- Tested rollback: v0.9 over v0.8 using a backup; `doctor` green both ways.

## Command surface

```
unpeel self-update --check [--manifest PATH] [--json]
unpeel self-update --apply [--manifest PATH] [--json]
```

- `--check`: compare installed version against the manifest; print
  whether an update is available. Exit 0 if up-to-date, exit 3 if an
  update is available (scriptable). No changes to the install.
- `--apply`: download (future; this phase: copy from local manifest
  dir), verify SHA-256, back up the current install, install the new
  binaries, run `unpeel doctor`. On any failure, restore the backup
  and re-run `doctor`.

## Manifest format (local file this phase; URL in a later phase)

```json
{
  "version": "0.9.0",
  "artifacts": {
    "x86_64-unknown-linux-gnu": {
      "url": "file:///path/to/unpeel-0.9.0-x86_64-unknown-linux-gnu.tar.gz",
      "sha256": "<hex>"
    }
  }
}
```

## Update flow (`--apply`)

1. Read manifest (local path this phase).
2. If installed version >= manifest version: exit 0, "already up to date".
3. Fetch artifact (this phase: local file copy; later: HTTPS with
   pinned hash).
4. Verify SHA-256 against the manifest. Mismatch → abort, no changes.
5. Back up current install: copy `$UNPEEL_HOME/bin` (or the install
   prefix) to `$UNPEEL_HOME/backups/self-update-<timestamp>/`.
   The backup is a full copy, not a symlink swap, so rollback is a
   plain recursive copy back.
6. Extract new binaries over the install prefix (atomic: extract to
   temp dir, then rename into place per binary).
7. Run `unpeel doctor`. If doctor fails → rollback: restore the backup,
   re-run doctor, report failure. Exit non-zero.
8. On success: keep the backup (prune backups older than N days on
   the next successful update). Print old → new version.

## Rollback (tested this phase)

- `unpeel self-update --rollback`: restore the most recent backup,
  re-run doctor. Refuses if no backup exists.
- The Phase 14 test: install v0.8 binaries, `--apply` a v0.9 manifest
  (local files), assert version is 0.9 and doctor is green; then
  `--rollback`, assert version is 0.8 and doctor is green.

## Safety properties

- **No network this phase.** The manifest path must be a `file://` URL
  or a plain local path; any `https://` manifest is rejected with a
  clear error ("network updates not enabled in this build").
- **Fail closed.** Any verification or doctor failure restores the
  backup before reporting. The install is never left half-updated:
  per-binary rename is atomic, and doctor gates the commit.
- **No credential handling.** The local-file flow takes no tokens;
  the future HTTPS flow will use the same token-via-env convention
  as the rest of the CLI (never argv).
- **Backup retention.** Keep the last 2 backups; prune older ones on
  successful update. Backups live under `$UNPEEL_HOME/backups/`, never
  in the install prefix.

## Out of scope (later phase)

- Actual HTTPS download, signature verification (sigstore/cosign),
  delta updates, background auto-check scheduling, Windows/macOS
  installer integration.
