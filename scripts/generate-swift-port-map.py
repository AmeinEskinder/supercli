#!/usr/bin/env python3
"""Generate docs/parity/swift-port-map.md: one row per legacy Swift file.

Walks the four Swift areas under clients/legacy (excluding vendored code),
assigns an initial port destination per file via filename heuristics, and
writes a Markdown table with Path | LOC | Destination | Status | Checklist
rows | Tests ported.

RE-RUNNABLE / IDEMPOTENT: when the map already exists, rows that show a
worker signal (Status != todo, non-empty Checklist rows, or Tests ported > 0)
are preserved verbatim. Pristine rows get their Destination refreshed from the
current heuristics (so heuristic improvements propagate) while Path and LOC are
always refreshed from disk. New files get heuristic destinations and `todo`
status. Files deleted from the tree are dropped from the map (reported on
stdout).

Workers: update the table, then re-run this script before committing so the
summary numbers stay real:
    python3 scripts/generate-swift-port-map.py

LOC = total lines per file (including blanks and comments). This matches the
historical "~147k lines" figure for the legacy Swift tree.
"""

import argparse
import os
import re
import sys

# Swift areas to walk, relative to clients/legacy.
SWIFT_AREAS = [
    "native/SupercliNative",
    "ios/SupercliIOS",
    "app-kit/swift",
    "shared/SupercliShared",
]

# Path fragments (case-insensitive) that mark vendored/third-party code.
# Our own integration files (e.g. GhosttyBridge.swift) are NOT excluded:
# only actual vendored trees are.
EXCLUDE_FRAGMENTS = ("vendor", "third-party", "third_party", "libghostty-spm")

STATUSES = ("todo", "ported", "wired", "verified")

# Destination labels.
D_RUST_SHARED = "Rust: supercli-client/core"
D_RUST_BRIDGE = "Rust: supercli-native-bridge"
D_DART_APP = "Dart: clients/supercli-app"
D_DART_APPKIT = "Dart: appkit_widgets"
D_RUST_LOGIC = "Rust: supercli-core/client"
D_RUST_IOS = "Rust: supercli-client/core (iOS non-UI)"
D_GAP_IOS_UI = "gpuidart gap: iOS UI (waits for gpuidart mobile)"
D_TBD = "tbd"

NATIVE_SERVICE_KW = (
    "Service", "Keychain", "Launchd", "Updater", "Sparkle", "Notification",
    "Notifier", "Finder", "MenuBar", "StatusItem", "LoginItem", "XPC",
    "License", "HookServer",
)
NATIVE_UI_KW = (
    "View", "Window", "Panel", "Popover", "Sheet", "Alert", "HUD", "Cell",
    "Overlay", "Toolbar", "Sidebar", "Controller", "Menu", "Icon", "Chrome",
    "Mascot",
    # Worker D (sidebar/session/workspace): client UI models and views.
    "Session", "Workspace", "Worktree", "Toast", "Picker", "Gallery",
    "Screenshot", "Capture", "Markup", "Registry", "Pool",
)
IOS_UI_KW = (
    "View", "Screen", "Sheet", "Cell", "Button", "Label", "Mascot", "App",
    "Gallery", "Annotation",
)
IOS_LOGIC_KW = (
    "Store", "Storage", "Pairing", "Presence", "State", "Model", "Manager",
    "Service", "Client", "Protocol", "Controller", "Reflection", "Settings",
    "Flags", "Record", "Connection", "Transport", "Prediction", "Socket",
    "Cache", "Reconciler", "Filter", "Query",
)
# Native non-UI logic that belongs in Rust (supercli-core/client), but is
# neither an OS service (bridge) nor UI (Dart app).
NATIVE_LOGIC_KW = (
    "Bridge", "Backend", "Client", "Proxy", "Store", "Models", "Config",
    "Flags", "State", "Cache", "Theme", "Adapter", "DTO", "Runtime",
    "Uplink", "Artifacts", "Snapshot", "Scope", "Rules", "Command",
    "Request", "Capabilities", "Hardware",
)


def contains_any(stem, keywords):
    return any(kw.lower() in stem.lower() for kw in keywords)


def heuristic_destination(rel_path):
    """Initial destination guess from the path relative to clients/legacy."""
    stem = os.path.splitext(os.path.basename(rel_path))[0]
    if rel_path.startswith("shared/SupercliShared/"):
        return D_RUST_SHARED
    if rel_path.startswith("app-kit/swift/"):
        return D_DART_APPKIT
    if rel_path.startswith("ios/SupercliIOS/"):
        if contains_any(stem, IOS_UI_KW):
            return D_GAP_IOS_UI
        if contains_any(stem, IOS_LOGIC_KW):
            return D_RUST_IOS
        return D_TBD
    if rel_path.startswith("native/SupercliNative/"):
        if contains_any(stem, NATIVE_SERVICE_KW):
            return D_RUST_BRIDGE
        if contains_any(stem, NATIVE_UI_KW):
            return D_DART_APP
        if contains_any(stem, NATIVE_LOGIC_KW):
            return D_RUST_LOGIC
        return D_TBD
    return D_TBD


def collect_swift_files(legacy_root):
    """Return {rel_path: loc} for in-scope Swift files, sorted by path."""
    files = {}
    for area in SWIFT_AREAS:
        area_root = os.path.join(legacy_root, area)
        if not os.path.isdir(area_root):
            print(f"WARNING: missing area dir {area_root}", file=sys.stderr)
            continue
        for dirpath, _dirnames, filenames in os.walk(area_root):
            for name in filenames:
                if not name.endswith(".swift"):
                    continue
                full = os.path.join(dirpath, name)
                rel = os.path.relpath(full, legacy_root).replace(os.sep, "/")
                lowered = rel.lower()
                if any(frag in lowered for frag in EXCLUDE_FRAGMENTS):
                    continue
                with open(full, "r", encoding="utf-8", errors="replace") as fh:
                    loc = sum(1 for _ in fh)
                files[rel] = loc
    return dict(sorted(files.items()))


def parse_existing_map(path):
    """Parse an existing map table -> {rel_path: (dest, status, checklist, tests)}."""
    existing = {}
    try:
        with open(path, "r", encoding="utf-8") as fh:
            lines = fh.read().splitlines()
    except FileNotFoundError:
        return existing
    in_table = False
    for line in lines:
        s = line.strip()
        if s.startswith("| Path |"):
            in_table = True
            continue
        if not in_table or not s.startswith("|"):
            continue
        if re.match(r"^\|[\s\-:|]+\|$", s):
            continue  # separator row
        cells = [c.strip() for c in s.strip("|").split("|")]
        if len(cells) != 6:
            continue
        rel, _loc, dest, status, checklist, tests = cells
        try:
            tests_n = int(tests)
        except ValueError:
            tests_n = 0
        existing[rel] = (dest, status, checklist, tests_n)
    return existing


def has_worker_signal(dest_status_checklist_tests):
    """True if a worker has touched this row (beyond the initial heuristic)."""
    _dest, status, checklist, tests = dest_status_checklist_tests
    return status != "todo" or bool(checklist.strip()) or tests != 0


def area_of(rel_path):
    for area in SWIFT_AREAS:
        if rel_path.startswith(area + "/"):
            return area
    return "other"


def fmt_pct(num, den):
    return f"{(100.0 * num / den):.1f}%" if den else "0.0%"


def render_map(files, rows, dropped):
    """files: {rel: loc}; rows: {rel: (dest, status, checklist, tests)}."""
    total_loc = sum(files.values())
    verified_loc = sum(loc for rel, loc in files.items()
                       if rows[rel][1] == "verified")

    status_files, status_loc = {}, {}
    dest_files, dest_loc = {}, {}
    for rel, loc in files.items():
        dest, status, _c, _t = rows[rel]
        status_files[status] = status_files.get(status, 0) + 1
        status_loc[status] = status_loc.get(status, 0) + loc
        dest_files[dest] = dest_files.get(dest, 0) + 1
        dest_loc[dest] = dest_loc.get(dest, 0) + loc

    area_files, area_loc = {}, {}
    for rel, loc in files.items():
        a = area_of(rel)
        area_files[a] = area_files.get(a, 0) + 1
        area_loc[a] = area_loc.get(a, 0) + loc

    def breakdown(files_d, loc_d):
        parts = []
        for key in sorted(files_d):
            parts.append(f"{key}: {files_d[key]} files ({loc_d[key]:,} LOC)")
        return " · ".join(parts)

    out = []
    out.append("# Swift port map")
    out.append("")
    out.append("One row per Swift file under `clients/legacy` (vendored code excluded).")
    out.append("Generated by `scripts/generate-swift-port-map.py` — re-run it before")
    out.append("committing so the summary numbers stay real. The script refreshes")
    out.append("Path/LOC from disk and preserves worker edits to Destination, Status,")
    out.append("Checklist rows, and Tests ported for files that still exist.")
    out.append("")
    out.append("`clients/legacy` is FROZEN: read-only until this map shows 100% verified.")
    out.append("Deleting it is Amein's call. Never modify `clients/gpuidart`; log gaps in")
    out.append("`docs/gpuidart-gaps-*.md`.")
    out.append("")
    out.append("Status meanings: `todo` not started · `ported` behaviour ported with tests")
    out.append("· `wired` ported code wired to real backend/UI and mounted · `verified`")
    out.append("wired plus real-window screenshot (UI) or conformance proof (non-UI).")
    out.append("LOC = total lines per file (blanks and comments included).")
    out.append("")
    out.append("## Summary")
    out.append("")
    out.append(f"- Total Swift files: {len(files):,}")
    out.append(f"- Total Swift LOC: {total_loc:,}")
    out.append(f"- Verified: {fmt_pct(verified_loc, total_loc)} of LOC "
               f"({verified_loc:,} / {total_loc:,} lines)")
    out.append(f"- Status breakdown: {breakdown(status_files, status_loc)}")
    out.append(f"- Destination breakdown: {breakdown(dest_files, dest_loc)}")
    out.append("")
    out.append("### By area")
    out.append("")
    out.append("| Area | Files | LOC |")
    out.append("|---|---|---|")
    for area in SWIFT_AREAS:
        out.append(f"| {area} | {area_files.get(area, 0)} | {area_loc.get(area, 0):,} |")
    out.append("")
    if dropped:
        out.append(f"_Note: {len(dropped)} file(s) present in the previous map no longer "
                   "exist on disk and were dropped._")
        out.append("")
    out.append("## Files")
    out.append("")
    out.append("| Path | LOC | Destination | Status | Checklist rows | Tests ported |")
    out.append("|---|---|---|---|---|---|")
    for rel, loc in files.items():
        dest, status, checklist, tests = rows[rel]
        out.append(f"| {rel} | {loc} | {dest} | {status} | {checklist} | {tests} |")
    out.append("")
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser(description="Generate docs/parity/swift-port-map.md")
    ap.add_argument("--root", default=None,
                    help="Repo root (default: parent of the scripts/ dir holding this file)")
    args = ap.parse_args()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    root = args.root or os.path.dirname(script_dir)
    legacy_root = os.path.join(root, "clients", "legacy")
    out_path = os.path.join(root, "docs", "parity", "swift-port-map.md")

    files = collect_swift_files(legacy_root)
    existing = parse_existing_map(out_path)

    rows = {}
    new_files = 0
    refreshed = 0
    for rel, _loc in files.items():
        if rel in existing and has_worker_signal(existing[rel]):
            # Worker has touched this row: preserve everything.
            rows[rel] = existing[rel]
        elif rel in existing:
            # Pristine row: refresh the heuristic destination, keep the rest.
            _dest, status, checklist, tests = existing[rel]
            rows[rel] = (heuristic_destination(rel), status, checklist, tests)
            refreshed += 1
        else:
            rows[rel] = (heuristic_destination(rel), "todo", "", 0)
            new_files += 1
    dropped = sorted(set(existing) - set(files))

    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write(render_map(files, rows, dropped))

    total_loc = sum(files.values())
    print(f"swift files: {len(files)}, total LOC: {total_loc}, "
          f"new rows: {new_files}, refreshed heuristic rows: {refreshed}, "
          f"dropped rows: {len(dropped)}")
    for rel in dropped:
        print(f"  dropped: {rel}")


if __name__ == "__main__":
    main()
