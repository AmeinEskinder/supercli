"""`supercli backup` / `supercli restore` against the real binaries.

Drives the one-shot verbs (no Host needed) on a private SUPERCLI_HOME:
backup produces a verifiable archive, restore reinstalls it, and the
refusals (existing state, tampered archive, destination inside the home)
come back as non-zero exits.
"""

import sys, os, json, shutil, tarfile, io
from types import SimpleNamespace

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, run_cli  # noqa: E402


def _seed(home):
    home.write_state({"settings": {"theme": "dark"}, "projects": [], "presets": []})
    sess = os.path.join(home.path("app-sessions"), "sess-cli")
    os.makedirs(sess, exist_ok=True)
    with open(os.path.join(sess, "manifest.json"), "w") as handle:
        handle.write('{"id":"sess-cli"}')
    with open(os.path.join(sess, "output.bin"), "wb") as handle:
        handle.write(b"fake pty bytes")
    with open(home.path("mobile", "devices.json"), "w") as handle:
        handle.write('{"devices":[]}')


def _tamper_data_member(archive, member_suffix, dst):
    """Rewrite the tar with one data member's bytes changed; the manifest
    keeps the original hash, so restore must fail hash verification."""
    with tarfile.open(archive, "r") as src:
        members = src.getmembers()
        data = {m.name: src.extractfile(m).read() for m in members if m.isfile()}
    for name in data:
        if name.endswith(member_suffix):
            data[name] = b"tampered-by-test"
    with tarfile.open(dst, "w", format=tarfile.GNU_FORMAT) as out:
        for m in members:
            info = tarfile.TarInfo(m.name)
            info.size = len(data[m.name]) if m.isfile() else 0
            info.mode = m.mode
            info.mtime = m.mtime
            info.type = m.type
            out.addfile(info, io.BytesIO(data[m.name]) if m.isfile() else None)


def _empty_dir(root):
    """A restore target with no Supercli state at all. (Home() seeds
    app-state.json + app-sessions/, which restore must refuse.)"""
    shutil.rmtree(root, ignore_errors=True)
    os.makedirs(root)
    return SimpleNamespace(root=root)


def body(case):
    home = case.home
    _seed(home)
    archive = os.path.join(home.root + "-artifacts", "backup.tar")
    os.makedirs(os.path.dirname(archive), exist_ok=True)

    backed = run_cli(home, ["backup", "--to", archive, "--json"])
    case.check("backup exits 0", backed.returncode == 0, backed.stderr[:300])
    try:
        report = json.loads(backed.stdout)
    except ValueError:
        report = {}
    case.check(
        "backup --json reports the archive",
        report.get("ok") is True
        and report.get("sessions") == 1
        and report.get("files", 0) >= 4
        and os.path.isfile(archive),
        backed.stdout[:300],
    )

    inside = run_cli(home, ["backup", "--to", home.path("backup.tar")])
    case.check(
        "backup refuses a destination inside the home",
        inside.returncode != 0 and "inside the backed-up home" in inside.stderr,
        (inside.stdout + inside.stderr)[:300],
    )
    case.check("refused backup writes nothing", not os.path.exists(home.path("backup.tar")))

    dest = _empty_dir(home.root + "-restore")
    restored = run_cli(dest, ["restore", "--from", archive, "--json"])
    case.check("restore exits 0", restored.returncode == 0, restored.stderr[:300])
    try:
        rreport = json.loads(restored.stdout)
    except ValueError:
        rreport = {}
    case.check(
        # sessions counts restored session dirs; chains_verified counts
        # review chains (the seed has no review log — chain coverage is in
        # supercli-core's backup tests).
        "restore --json reports sessions",
        rreport.get("ok") is True and rreport.get("sessions") == 1,
        restored.stdout[:300],
    )
    with open(home.path("app-state.json"), "rb") as a, open(
        os.path.join(dest.root, "app-state.json"), "rb"
    ) as b:
        case.check("restore reinstalls byte-identical state", a.read() == b.read())
    case.check(
        "restore reinstalls pairing state",
        os.path.isfile(os.path.join(dest.root, "mobile", "devices.json")),
    )

    again = run_cli(dest, ["restore", "--from", archive])
    case.check(
        "restore refuses existing state without --force",
        again.returncode != 0 and "existing Supercli state" in again.stderr,
        (again.stdout + again.stderr)[:300],
    )

    tampered = os.path.join(home.root + "-artifacts", "tampered.tar")
    _tamper_data_member(archive, "output.bin", tampered)
    dest2 = _empty_dir(home.root + "-restore2")
    bad = run_cli(dest2, ["restore", "--from", tampered])
    case.check(
        "restore refuses a tampered archive",
        bad.returncode != 0 and "does not match the manifest" in bad.stderr,
        (bad.stdout + bad.stderr)[:300],
    )
    case.check(
        "tampered restore installs nothing",
        not os.path.exists(os.path.join(dest2.root, "app-sessions")),
    )

    help_out = run_cli(home, ["backup", "--help"])
    case.check("backup --help exits 0", help_out.returncode == 0)


run("backup", body)
