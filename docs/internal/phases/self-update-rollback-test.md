# Phase 14 (4): Rollback concept test — manual steps + result

Date: 2026-09-26 UTC. Private UNPEEL_HOME=/tmp/selfupdate-test-home
(no real ~/.unpeel touched).

## Steps (all executed, real output)

1. Fake v0.8 install:
   ```
   echo "unpeel-binary-v0.8-fake" > $UNPEEL_HOME/bin/unpeel
   sha256sum $UNPEEL_HOME/bin/unpeel
   # 9ac18226cc6effce82cedb760c1e4c9130ba5443419cb61de0e458258449b3e2
   ```

2. Backup (full copy, per docs/self-update-design.md):
   ```
   cp -a $UNPEEL_HOME/bin $UNPEEL_HOME/backups/self-update-<timestamp>
   ```

3. Simulate v0.9 install (copy new binary over):
   ```
   echo "unpeel-binary-v0.9-fake" > $UNPEEL_HOME/bin/unpeel
   sha256sum $UNPEEL_HOME/bin/unpeel
   # 8dae766fd691d887ec632b3e789857b913e42a074c0f576b96b5cd4c56efd80e
   ```
   Sanity: v0.8 and v0.9 hashes differ (the install actually changed
   something).

4. Rollback (plain recursive copy back from backup):
   ```
   cp -a $UNPEEL_HOME/backups/self-update-<timestamp>/. $UNPEEL_HOME/bin/
   ```

5. Verify:
   ```
   sha256sum $UNPEEL_HOME/bin/unpeel
   # 9ac18226cc6effce82cedb760c1e4c9130ba5443419cb61de0e458258449b3e2
   diff /tmp/v08.hash /tmp/v08-restored.hash
   # identical -> ROLLBACK VERIFIED
   ```

## Result

PASS. The backup-then-copy-back rollback restores the exact v0.8
binary (SHA-256 match). This validates the design doc's rollback
mechanism at the filesystem level.

## What this does NOT prove (UNTESTED)

- The real `--apply` flow (not implemented this phase).
- `doctor` green before/after (doctor not run against these fake
  binaries; the design requires it, the implementation must gate on it).
- Atomic per-binary rename under a concurrent reader.
- Backup pruning / retention.
