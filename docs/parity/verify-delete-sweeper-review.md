# Verify-Delete Sweeper — Review Note

**Branch:** `feat/verify-delete-sweeper`
**Base:** `origin/next@7f8f4e4`
**Goal:** 0% Swift. This batch targets the free deletions: manifests, demo, dead code.

## Deleted in this batch (2 files, 126 LOC)

| File | LOC | Reason | Callers | CI impact |
| ---- | --- | ------ | ------- | --------- |
| `clients/legacy/native/SupercliNative/Package.swift` | 91 | SwiftPM manifest for the frozen Mac app. Header calls it a "Phase 0 spike". The Mac app build is skipped in CI (`if: false`). Follows batch6 precedent (deleted `app-kit/swift/Package.swift` as "SwiftPM manifest, build file"). | None — manifest only, not imported by Swift code. | None — the unsigned Mac app job is skipped; the SwiftPM cache key references the vendor Package.swift, not this one. |
| `clients/legacy/ios/SupercliIOS/Package.swift` | 35 | SwiftPM manifest for the frozen iOS app. The iOS simulator test job is skipped in CI (`if: false`). Follows batch6 precedent. | None — manifest only. | None — iOS package tests are skipped. |

Both files are marked `pending` in `docs/parity/swift-deleted.md`.

## Surveyed but NOT deleted (needs reviewer call)

### Vendor libghostty-spm (111 Swift files)
**Location:** `clients/legacy/native/vendor/libghostty-spm/`
**Map status:** All rows marked `dropped: vendored libghostty-spm, deleted with app`, status `todo`.
**Why not deleted:** The map says it "goes away with it under the Swift-0% goal", but the GhosttyBridge port (`feat/port-ghosttybridge-b`) is actively using this as a reference implementation. Deleting 111 third-party files now would remove the reference while the port is in flight. This needs an explicit reviewer decision: delete now vs. delete with the app.
**Recommendation:** Hold until GhosttyBridge port merges, then delete as a single "delete-with-app" batch.

### dmg-background.swift (122 LOC)
**Location:** `clients/legacy/native/dmg-background.swift`
**Map status:** `todo`, destination `Rust: supercli-native-bridge`
**Why not deleted:** Standalone build script (`swift dmg-background.swift <out-dir>`) used by `make-dmg.sh` to render the DMG background. The Mac app DMG build is skipped in CI, so it's currently unused. However, it's a build tool, not app code — deleting it is a different decision from deleting app sources.
**Recommendation:** Reviewer call — delete as "build tool, Mac app not shipped" or keep.

### Crates Swift test clients (106 LOC total)
- `crates/supercli-cli/tests/pairclient/main.swift` (42 LOC)
- `crates/supercli-cli/tests/relayclient/main.swift` (64 LOC)
**Map status:** Both `todo`, destination `Rust: supercli-core/client`.
**Why not deleted:** Marked for Rust porting. May still be used by the Rust test harness. Not clearly "dead".
**Recommendation:** Leave for the porter fleet or a dedicated check.

### Dead-code analysis results
Ran a systematic check: extracted top-level type declarations from all non-vendor, non-test Swift files and verified each type has external references via `git grep`. Findings:
- **Confirmed dead (already in batch7):** `HostHardware.swift`, `RemoteUnpeelClient.swift` — both already marked `pending` by the batch7 worker. Not duplicated here.
- **False positives (not dead):**
  - `app-kit/swift` views (`CanvasPageView`, `MediaView`, `TextBoxView`, `TreeView`) — these are the UIProtocol port sources, actively being ported to Dart by the uiprotocol workers.
  - `dioxus/native-shell` bridges (`UnpeelPushBridge`, `UnpeelSpeechBridge`) — called from Rust/Dart via FFI, not from Swift. Marked `todo` for `supercli-native-bridge` port.
  - `ios/SupercliIOS/App/UnpeelIOSApp.swift` — iOS app entry point (`@main`), not referenced by other Swift files by design.
- **No other dead files found** in the native app, iOS app, or shared package.

## Ledger updates
- 2 rows appended to `docs/parity/swift-deleted.md` with status `pending`.
- Map regenerated with `scripts/generate-swift-port-map.py`; `--check` passes.

## Verification
- `python3 scripts/generate-swift-port-map.py --check`: PASS
- `bash scripts/fresh-clone-verify.sh --local .`: NOT RUN (disk full; 843M free, script needs more)
- No cargo work needed (manifest deletions only).
- Deletions verified: `git status` shows exactly 2 deleted files, no other modifications.
