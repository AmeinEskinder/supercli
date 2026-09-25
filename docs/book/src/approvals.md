# Approvals

When an agent wants to perform a side-effecting action (run a command,
write a file, call a connector), the Host asks for approval first. The
approval appears on every paired Controller device.

## The approval card

Each approval shows:

- **What** the agent wants to do (the tool and its arguments)
- **Where** it will run (session, working directory)
- **Who** asked (the actor: a human device, a scheduled trigger, or policy)

## Approve, deny, cancel

- **Approve** — the action runs. The decision and a hash of the arguments
  are written to the tamper-evident review log *before* the tool executes
  (write-ahead). If the review write fails, the action does not run.
- **Deny** — the action is blocked and the denial is logged.
- **Cancel** — the in-flight action is cancelled. Cancellation is
  best-effort: an action that already completed keeps its recorded outcome
  and is never overwritten.

## Uncertain outcomes

If the Host cannot tell whether an action ran (crash mid-flight, transport
dropped after send), the review is marked **Ambiguous** and surfaced for
human review. Ambiguous actions are never auto-retried.

A related case: if a PTY write was delivered but the confirmation was lost,
the retry resolves as **OutcomeUnknown** — surfaced for review, never
re-delivered.

## Review log

Every decision lands in `app-sessions/<id>/action-reviews.jsonl`, hash-chained
(each entry carries the hash of the previous one). Verify the chain with:

```sh
supercli doctor
```

See [Doctor and troubleshooting](doctor.md) for what to do when the chain
fails verification.
