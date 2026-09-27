# Wire fixtures — Rust ↔ Dart ↔ Host drift guard

One canonical JSON example per message struct in
`crates/supercli-client/src/dto.rs` (the Swift `RemoteControlProtocol.swift`
ports). These files are the shared contract between the Rust client, the Dart
app, and the Host.

## Provenance

The **values** in each fixture come from the Swift XCTest expectations in
`clients/legacy/shared/SupercliShared/Tests/SupercliSharedTests/RemoteControlProtocolTests.swift`
(each fixture's source test is noted in the `b_*` builder in
`crates/supercli-client/tests/wire_fixtures.rs`). The **bytes** are the Rust
serializer's canonical form: compact JSON, fields in declaration order, all
fields present (including explicit `null`s).

## The contract

- **Rust** (`crates/supercli-client/tests/wire_fixtures.rs`,
  `wire_fixtures_decode_and_reencode_byte_stable`): every fixture must decode
  into its DTO *and* re-encode to byte-identical JSON. Any Rust-side wire
  change — renamed field, new enum spelling, added/removed field — breaks CI
  here instead of the app at runtime.
- **Dart** (`clients/supercli-app/test/wire_fixtures_test.dart`): every
  fixture must decode through `host_models.dart` (wire DTOs) without throwing,
  with key fields asserted.

## Changing the wire format

1. Make the DTO change in Rust (`dto.rs`) and/or Dart (`host_models.dart`).
2. Regenerate: `cargo test -p supercli-client --test wire_fixtures -- --ignored`
3. Review the `git diff` of this directory — it should show exactly the
   intended wire change and nothing else.
4. Both test suites must be green before merge.

## Notes

- `project_summary.json` uses `isGroup`: the Host's sidebar (`sessions.rs`)
  and Swift's `RemoteControlProtocol` both use `isGroup`. The legacy macOS
  native client reads snake_case `is_folder` from a different store format
  (frozen, untouched).
- `terminal_write_request.json` uses `wid` for the idempotency key: the Host
  (`controller_api.rs` `write_session`, `remote_server.rs`) reads `wid`, and
  Swift's `writeID` property encodes as `wid` on the wire.
- `terminal_resize_request.json` uses `columns`: the Host's `resize_session`
  reads `columns`, and Swift uses `columns`.
- `viewport_patch.json` uses `column` for cell runs, matching Swift's
  `RemoteTerminalCellRun` (the Host does not emit viewport patches).
- Fixtures never contain secrets or real user data; all ids/timestamps are
  taken from the Swift tests.
