# Phase 14 (5): Simulated day — plan

## Goal

Run ~40 realistic actions over 3 sessions through the real Host and a
paired controller, covering: reads, writes, a denial, a cancellation,
a rate-limit burst, and a mid-day Host restart. Fix the top 5 UX
frictions found, then rerun.

## Session plan

### Session A — "Morning triage" (~14 actions)
1. Bootstrap + list sessions
2. Read transcript of overnight session
3. Approve a file write (allow)
4. Deny a network fetch (deny) — verify denial UX
5. Read a file via controller
6. Send a message to the session
7. Check approval queue (empty)
8. Trigger a rate-limit burst: 5 rapid approval answers (429s expected)
9. Verify retry-after respected, all 5 eventually applied
10. Cancel an in-flight approval
11. Verify cancellation UX
12. List artifacts
13. Read session activity
14. End session

### Session B — "Deep work" (~14 actions)
1. Create new session via controller
2. Send complex task prompt
3. Approve 3 sequential tool writes
4. Read output after each
5. Test pairing code display
6. Simulate slow network (delayed responses)
7. Verify timeout UX
8. Deny a destructive action (rm -rf)
9. Verify deny is terminal (no retry prompt)
10. Check audit log has the denial entry
11. Test search in transcript
12. Archive the session
13. Verify archived session is readable
14. End session

### Session C — "Afternoon + restart" (~12 actions)
1. Bootstrap
2. Start a long-running task
3. **Mid-day Host restart** (kill -9, restart)
4. Verify sessions survive restart (manifest/journal replay)
5. Answer an approval that was pending before restart → expect 409
6. Verify phone shows "Resolved — see activity log" (not Failed)
7. Create new approval after restart
8. Answer it normally (200 OK)
9. Verify idempotent retry works (AlreadyResolved)
10. Check doctor is green after restart
11. Verify no duplicate audit entries from the restart
12. End session

## Friction log

Record every UX friction encountered (confusing message, missing
feedback, slow response, unclear state). After the run, rank by
severity × frequency, fix the top 5, rerun the full 40-action suite.

## Success criteria

- All 40 actions complete without manual intervention
- Zero errors (excluding expected 429s and the 409-after-restart)
- Top 5 frictions fixed and verified on rerun
- Audit log has exactly one entry per approval decision
- Doctor green before and after
