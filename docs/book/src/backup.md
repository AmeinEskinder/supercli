# Backup and restore

## Backup

`supercli backup` takes a consistent snapshot of your workspace:

```sh
supercli backup --output supercli-backup.tar.gz
```

The snapshot covers sessions, artifacts, the lease database (via SQLite
online backup), mobile pairing state, and pane layouts. A SHA-256 manifest
is embedded in the archive so restores can verify integrity.

Stop the Host before backing up — the snapshot takes the same lock the
Host's writers hold, so a running Host will refuse the backup rather than
produce a torn snapshot.

## Restore

```sh
supercli restore --input supercli-backup.tar.gz
```

Restore:

1. Verifies the SHA-256 manifest before touching anything.
2. Refuses to run while a Host is using the destination home.
3. Refuses a destination inside the source home.
4. Re-verifies the review hash chain after installing.

Restores are transactional: if verification fails, the destination is left
untouched.
