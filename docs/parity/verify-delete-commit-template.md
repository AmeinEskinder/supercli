# Verify-delete commit body template

Every verify-delete branch (Swift file deletion after a port is proven) must
end its commit body with a REVIEW NOTE block. Copy this template verbatim,
filling in the bracketed parts:

```
REVIEW NOTE: <N> frozen Swift files still reference <Type> (<file list>).
Deletion branch is review-blocked; deletion decisions are Claude's review +
Amein's, and nobody else's.
```

Rules:

- The authority line is exactly: "deletion decisions are Claude's review +
  Amein's, and nobody else's."
- Do not name any other approver (human, agent, or otherwise) in the commit
  body. A stale "Claude/Osman" wording existed in batch3's commit message;
  it is superseded by this template.
- The "frozen Swift files still reference" list is expected: the frozen
  native app stops compiling until the referencing files are ported (same
  pattern as batches 1-3). List them honestly; do not delete a file whose
  port is in doubt ("when in doubt, leave it").
