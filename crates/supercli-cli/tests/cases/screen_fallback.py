"""The screen fallback tier: a recognized agent with no Supercli integration
animates busy/idle from its screen, never notifies, and yields to hooks.

A fake `claude` (no integration installed in the private HOME) draws Claude's
real spinner line, then its idle prompt. The Host classifies the viewport with
the runtime's [screen] rules; the worker publishes the verdict with
activity_source "screen" and records no "finished" activity for the edge.
"""

import json
import os
import re
import shlex
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, run_cli  # noqa: E402
from hook_cancellation import control, fixture_started, read_json  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "hooks", home.root)
    environment = {"HOME": home.root, "SHELL": "/bin/bash",
                   "PATH": home.path("bin") + os.pathsep + os.environ["PATH"]}
    os.makedirs(home.path("bin"), exist_ok=True)
    fake = home.path("bin", "claude")
    with open(fake, "w") as handle:
        handle.write(f"#!{sys.executable}\n" + '''
import os, tty
tty.setraw(0)
def screen(text):
    print("\\x1b[2J\\x1b[H" + text.replace("\\n", "\\r\\n"), end="", flush=True)
print("HOOK_FIXTURE_PROCESS_STARTED", flush=True)
screen("Claude Code\\n\\n✻ Levitating… (12s · ↓ 5.1k tokens)\\n────\\n❯\\n────\\n  ⏵⏵ auto mode on")
while True:
    data = os.read(0, 4096)
    if not data: break
    if data == b"d":
        screen("Claude Code\\n\\n✻ Brewed for 12s · done\\n────\\n❯\\n────\\n  ⏵⏵ auto mode on")
    elif data == b"w":
        screen("Claude Code\\n\\n✽ Thinking… (2s · esc to interrupt)\\n────\\n❯\\n────\\n  ⏵⏵ auto mode on")
    elif data == b"q":
        break
''')
    os.chmod(fake, 0o755)
    home.preset(label="claude-screen", command=shlex.quote(fake), preset_id="claude-screen")
    service = case.serve(env=environment)
    if not case.check("isolated Host starts", bool(service.ready(timeout=25)), service.log()):
        return
    listed = run_cli(home, ["integrations", "list", "--json"], env=environment)
    rows = json.loads(listed.stdout or "[]")
    case.check("no Claude integration is installed in the private HOME",
               any(row["runtime"] == "claude" and not row["installed"] for row in rows), listed.stdout[:200])
    launched = run_cli(home, ["new", "--preset", "claude-screen", "--project", "p"], env=environment)
    ids = re.findall(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}", launched.stdout)
    if not case.check("fake Claude launches", launched.returncode == 0 and bool(ids), launched.stderr):
        return
    sid = ids[0]
    session_dir = home.path("app-sessions", sid)
    if not fixture_started(case, service, session_dir):
        return

    def activity():
        return read_json(home.path("activity-state.json")).get("sessions", {}).get(sid, {})

    def manifest_verdict():
        return read_json(os.path.join(session_dir, "manifest.json")).get("screen_activity")

    try:
        case.check("the Host classifies the spinner screen as working",
                   bool(service.wait_for(lambda: manifest_verdict() == "working", timeout=15)), str(manifest_verdict()))
        case.check("the worker reports busy from the screen tier",
                   bool(service.wait_for(lambda: activity().get("raw_status") == "busy"
                                         and activity().get("activity_source") == "screen", timeout=10)), str(activity()))
        control(home, sid, "d")
        case.check("the idle prompt classifies as idle",
                   bool(service.wait_for(lambda: manifest_verdict() == "idle", timeout=15)), str(manifest_verdict()))
        case.check("the worker reports idle from the screen tier",
                   bool(service.wait_for(lambda: activity().get("raw_status") == "idle"
                                         and activity().get("activity_source") == "screen", timeout=10)), str(activity()))
        case.check("a screen-derived idle never counts as a completed turn", activity().get("completed") is not True, str(activity()))
        service.read_for(1.0)
        finished = []
        try:
            with open(home.path("activity-log.jsonl")) as handle:
                for line in handle:
                    entry = json.loads(line)
                    if entry.get("session_id") == sid and entry.get("kind") in ("finished", "Finished"):
                        finished.append(entry)
        except (OSError, ValueError):
            pass
        case.check("a screen-derived edge records no finished activity (no notification)", not finished, str(finished)[:200])
        control(home, sid, "w")
        case.check("a new spinner flips back to busy",
                   bool(service.wait_for(lambda: activity().get("raw_status") == "busy", timeout=15)), str(activity()))
    finally:
        control(home, sid, "q")
        run_cli(home, ["rm", sid], env=environment, timeout=45)


run("screen_fallback", body)
