# Action Review Hash Chain — Canonical Form

This document specifies the exact byte-level canonical form used for the
tamper-evident hash chain in `action-reviews.jsonl`. Any implementation that
writes or verifies this log (including the D1 control plane) must produce
byte-identical canonical form, or hash verification will fail.

## Overview

Each entry in `action-reviews.jsonl` carries an `entry_hash`: the SHA-256 hex
digest of the entry's canonical bytes. Each entry (except the genesis) also
carries `prev_hash`: the `entry_hash` of the previous entry. This forms a
hash chain.

Two levels of verification make the chain tamper-evident (added 2026-09-25,
fixing a hole where renamed keys / whitespace / equivalent escapes in a
stored line were invisible to verification):

1. **Byte-equality.** The verifier parses each stored line, re-serializes the
   parsed entry with the writer's canonical serializer, and requires the
   result to byte-equal the stored line. Any deviation — renamed key,
   added whitespace, `\uXXXX` escape in place of a literal char, key
   reordering — is rejected as `ChainError::NonCanonical` before any hash
   is checked. The accepted language is therefore *exactly* the set of
   writer-produced byte strings.

2. **Hash + link check.** `entry_hash` must equal
   `hex(sha256(canonical_bytes))` of the parsed entry, and each entry's
   `prev_hash` must equal the previous entry's `entry_hash`.

Together: flipping any byte of any accepted entry breaks verification.
(The writer cannot hash literally the bytes it writes — `entry_hash` is part
of the line — but by (1) the hashed canonical bytes are a pure deterministic
function of the on-disk line, so the chain is defined over the on-disk
bytes.)

Two entry types exist:
- **Review entries** (`ReviewEntry`): written before a tool executes (write-ahead).
- **Outcome entries** (`OutcomeEntry`, `"type": "attempt_outcome"`): written after
  the tool attempt resolves (Executed / Ambiguous / NeverRan).

## Canonical JSON

The canonical bytes are the UTF-8 encoding of a JSON object with these properties:

1. **No `entry_hash` field.** The hash is computed over the entry *without*
   its own `entry_hash` (it is what we are computing).

2. **Keys sorted alphabetically** (byte-wise, by UTF-8 code unit). The Rust
   implementation builds the object with `serde_json::json!` and serializes
   with `to_string()`. `serde_json` is built **without** the `preserve_order`
   feature, so `Map` is a `BTreeMap` and keys emit in sorted order regardless
   of insertion order in the macro.

3. **No whitespace.** `to_string()` emits the most compact form: no spaces
   after `:` or `,`.

4. **String escaping** follows `serde_json`'s `to_string()`: `"` → `\"`,
   `\` → `\\`, control chars → `\u00XX`, etc. Non-ASCII Unicode is emitted
   as raw UTF-8 (not `\u` escaped).

5. **Numbers** are emitted as-is (integers without decimal point).

6. **`null`** is emitted for `None` / missing optional values (e.g. `"success": null`
   on non-Executed outcomes, `"reason": null` on Executed).

### Review entry fields

| Field | Type | Notes |
|---|---|---|
| `actor` | string | e.g. `"human:paired-device"`, `"policy:allow"`, `"scheduled:<id>"` |
| `args_hash` | string | hex SHA-256 of the tool arguments |
| `connector` | string | connector id |
| `decision` | string | `"approved"` or `"denied"` |
| `prev_hash` | string | hex SHA-256 of previous entry's canonical bytes; `"genesis"` (lowercase) for the first |
| `replaces_attempt` | string \| null | review id being replaced, or null |
| `review_id` | string | UUID of this review |
| `tool` | string | tool name |
| `ts_ms` | integer | Unix milliseconds |

Sorted key order: `actor`, `args_hash`, `connector`, `decision`, `prev_hash`,
`replaces_attempt`, `review_id`, `tool`, `ts_ms`.

### Outcome entry fields

| Field | Type | Notes |
|---|---|---|
| `actor` | string | actor string, same vocabulary as review entries |
| `outcome` | string | `"executed"`, `"ambiguous"`, or `"never_ran"` |
| `prev_hash` | string | hex SHA-256 of previous entry's canonical bytes |
| `reason` | string \| null | human-readable reason; null for Executed |
| `review_id` | string | UUID of the review this outcome resolves |
| `success` | boolean \| null | true/false for Executed; null otherwise |
| `ts_ms` | integer | Unix milliseconds |
| `type` | string | always `"attempt_outcome"` |

Sorted key order: `actor`, `outcome`, `prev_hash`, `reason`, `review_id`,
`success`, `ts_ms`, `type`.

## Hash

`entry_hash = hex(sha256(canonical_bytes))`, lowercase hex.

## Golden vectors

Fixed inputs and their exact canonical bytes + hashes are pinned in
`crates/unpeel-core/src/action_reviews.rs`:

- `golden_vector_canonical_bytes`: pins `canonical_bytes()` output and its
  SHA-256 for a fixed review entry (keys alphabetical, no whitespace, no
  `entry_hash`).
- `golden_vector_stored_line`: pins the exact stored line the writer emits
  for a genesis review entry (`"prev_hash": "genesis"`, with `entry_hash`
  present and sorted between `decision` and `prev_hash`), and asserts the
  verifier accepts it. Any reimplementation must reproduce those bytes
  exactly.

## Rationale

Alphabetical key order was chosen because it is deterministic without
requiring the `preserve_order` feature (which would make the hash depend on
code-level insertion order — fragile across refactors and languages). Any
JSON implementation can sort keys; the canonical form is thus portable.
