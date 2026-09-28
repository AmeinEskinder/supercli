#!/usr/bin/env python3
"""Generate docs/parity/swift-port-map.md: one row per legacy Swift file.

Walks the four Swift areas under clients/legacy (excluding vendored code),
assigns an initial port destination per file via filename heuristics, applies
per-area sidecar overrides from docs/parity/swift-port/<area>.yml, and writes
a Markdown table with Path | LOC | Destination | Status | Checklist rows |
Tests ported.

SIDECAR WORKFLOW (no more merge conflicts):
  - Workers NEVER hand-edit docs/parity/swift-port-map.md. It is generated.
  - Each worker area owns docs/parity/swift-port/<area>.yml. Edit ONLY your
    area's file. Two workers never touch the same file.
  - Sidecar fields per Swift file: destination, status, checklist, behaviours,
    tests, notes. The sidecar WINS over the heuristic for every field it sets.
  - Re-run this script after editing your sidecar, then commit both the yml
    and the regenerated md:
        python3 scripts/generate-swift-port-map.py

RE-RUNNABLE / IDEMPOTENT: with no sidecar changes the output is
byte-identical. Path/LOC are always refreshed from disk. Files deleted from
the tree are dropped from the map (reported on stdout); sidecar entries for
missing files are reported as warnings but kept.

STRICT "ported" STANDARD: a row is `ported` only when EVERY Swift behaviour
(func, gesture, keybinding, state transition) maps to a Dart/Rust function
PLUS a test. List behaviours in the sidecar; `partial` means some behaviours
are ported. Zero tests means not ported.

LOC = total lines per file (including blanks and comments). This matches the
historical "~147k lines" figure for the legacy Swift tree.
"""

import argparse
import os
import re
import sys

try:
    import yaml
except ImportError:
    print("ERROR: PyYAML is required (pip install pyyaml)", file=sys.stderr)
    sys.exit(2)

# Swift areas to walk, relative to clients/legacy.
SWIFT_AREAS = [
    "native/SupercliNative",
    "ios/SupercliIOS",
    "app-kit/swift",
    "shared/SupercliShared",
]

# Path fragments (case-insensitive) that mark vendored/third-party code.
EXCLUDE_FRAGMENTS = ("vendor", "third-party", "third_party", "libghostty-spm")

# Worker areas, each owning docs/parity/swift-port/<area>.yml.
WORKER_AREAS = (
    "shared",
    "macos-services",
    "terminal",
    "sidebar",
    "settings",
    "remote",
    "appkit",
    "ios",
    "rootview",
)

STATUSES = ("todo", "partial", "ported", "wired", "verified")

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
    # Worker C (terminal/pane): terminal UI surface.
    "Terminal", "Pane", "FindBar", "DropTarget", "DragMap",
    # Worker D (sidebar/session/workspace): client UI models and views.
    "Session", "Workspace", "Worktree", "Toast", "Picker", "Gallery",
    "Screenshot", "Capture", "Markup", "Registry", "Pool",
)
IOS_UI_KW = (
    "View", "Screen", "Sheet", "Cell", "Button", "Label", "Mascot", "App",
    "Gallery", "Annotation",
    # Worker C: iOS canvas layout is UI.
    "Canvas", "Layout",
)
IOS_LOGIC_KW = (
    "Store", "Storage", "Pairing", "Presence", "State", "Model", "Manager",
    "Service", "Client", "Protocol", "Controller", "Reflection", "Settings",
    "Flags", "Record", "Connection", "Transport", "Prediction", "Socket",
    "Cache", "Reconciler", "Filter", "Query",
    # Worker C: input tracking is protocol logic, not UI.
    "Tracker", "Mouse",
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


def worker_area_of(rel_path):
    """Best-effort mapping of a Swift file to a worker area, for summaries.

    Sidecar claims are authoritative; this is only used to group unclaimed
    files so the tbd-per-area counts stay meaningful.
    """
    if rel_path.startswith("shared/SupercliShared/"):
        return "shared"
    if rel_path.startswith("app-kit/swift/"):
        return "appkit"
    if rel_path.startswith("ios/SupercliIOS/"):
        return "ios"
    if rel_path.startswith("native/SupercliNative/"):
        stem = os.path.splitext(os.path.basename(rel_path))[0]
        low = stem.lower()
        # Order matters: most specific first.
        if contains_any(stem, ("Terminal", "Pane", "FindBar", "DropTarget",
                               "DragMap", "Ghostty")):
            return "terminal"
        if contains_any(stem, ("Relay", "Link", "Pairing", "Remote",
                               "Connection", "Transport")):
            return "remote"
        if "Settings" in stem or "Panel" in stem or "Plugin" in stem \
                or "Preset" in stem or "Browser" in stem:
            return "settings"
        if contains_any(stem, ("Sidebar", "Session", "Workspace", "Worktree",
                               "Toast", "Picker", "Gallery")):
            return "sidebar"
        if contains_any(stem, NATIVE_SERVICE_KW):
            return "macos-services"
        return "macos-services"  # native leftovers default here
    return "shared"


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


def load_sidecars(sidecar_dir):
    """Load all <area>.yml -> {rel_path: entry}. Validates area names."""
    claimed = {}
    if not os.path.isdir(sidecar_dir):
        return claimed
    for fname in sorted(os.listdir(sidecar_dir)):
        if not fname.endswith(".yml"):
            continue
        area = fname[:-4]
        if area not in WORKER_AREAS:
            print(f"WARNING: unknown sidecar area '{area}' in {fname}; "
                  f"expected one of {WORKER_AREAS}", file=sys.stderr)
            continue
        path = os.path.join(sidecar_dir, fname)
        with open(path, "r", encoding="utf-8") as fh:
            data = yaml.safe_load(fh) or {}
        if data.get("area", area) != area:
            print(f"WARNING: {fname} declares area '{data.get('area')}', "
                  f"expected '{area}'", file=sys.stderr)
        for rel, entry in (data.get("files") or {}).items():
            if not isinstance(entry, dict):
                print(f"WARNING: {fname}: entry for {rel} is not a mapping; "
                      "skipped", file=sys.stderr)
                continue
            status = entry.get("status", "todo")
            if status not in STATUSES:
                print(f"WARNING: {fname}: {rel} has unknown status "
                      f"'{status}'; expected one of {STATUSES}", file=sys.stderr)
            if rel in claimed:
                print(f"ERROR: {rel} claimed by both "
                      f"'{claimed[rel][0]}' and '{area}' sidecars",
                      file=sys.stderr)
                sys.exit(1)
            claimed[rel] = (area, entry)
    return claimed


def tests_ported_count(entry):
    """Count ported tests from a sidecar entry's `tests` list."""
    total = 0
    for item in entry.get("tests") or []:
        if isinstance(item, dict):
            total += int(item.get("count", 1))
        else:
            total += 1  # plain string = one test file, count 1
    return total


def render_map(files, rows, dropped, stale_sidecars):
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
        a = worker_area_of(rel)
        area_files[a] = area_files.get(a, 0) + 1
        area_loc[a] = area_loc.get(a, 0) + loc

    def breakdown(files_d, loc_d):
        parts = []
        for key in sorted(files_d):
            parts.append(f"{key}: {files_d[key]} files ({loc_d[key]:,} LOC)")
        return " · ".join(parts)

    tbd_total = dest_files.get(D_TBD, 0)

    out = []
    out.append("# Swift port map")
    out.append("")
    out.append("One row per Swift file under `clients/legacy` (vendored code excluded).")
    out.append("Generated by `scripts/generate-swift-port-map.py` — DO NOT hand-edit.")
    out.append("Workers edit only their `docs/parity/swift-port/<area>.yml` sidecar,")
    out.append("then re-run the script so the summary numbers stay real. Sidecar values")
    out.append("win over the filename heuristics for every field they set.")
    out.append("")
    out.append("`clients/legacy` is FROZEN: read-only until this map shows 100% verified.")
    out.append("Deleting it is Amein's call. Never modify `clients/gpuidart`; log gaps in")
    out.append("`docs/gpuidart-gaps-*.md`.")
    out.append("")
    out.append("Status meanings: `todo` not started · `partial` some behaviours ported")
    out.append("· `ported` every behaviour ported with tests (strict: each Swift func,")
    out.append("gesture, keybinding and state transition maps to a Dart/Rust function")
    out.append("PLUS a test; zero tests means not ported) · `wired` ported code wired to")
    out.append("real backend/UI and mounted · `verified` wired plus real-window")
    out.append("screenshot (UI) or conformance proof (non-UI).")
    out.append("LOC = total lines per file (blanks and comments included).")
    out.append("Behaviour-level detail lives in the per-area sidecars under")
    out.append("`docs/parity/swift-port/`.")
    out.append("")
    out.append("## Summary")
    out.append("")
    out.append(f"- Total Swift files: {len(files):,}")
    out.append(f"- Total Swift LOC: {total_loc:,}")
    out.append(f"- Verified: {fmt_pct(verified_loc, total_loc)} of LOC "
               f"({verified_loc:,} / {total_loc:,} lines)")
    out.append(f"- Status breakdown: {breakdown(status_files, status_loc)}")
    out.append(f"- Destination breakdown: {breakdown(dest_files, dest_loc)}")
    out.append(f"- Unresolved (`tbd`) destinations: {tbd_total} files")
    out.append("")
    out.append("### By worker area")
    out.append("")
    out.append("| Area | Files | LOC |")
    out.append("|---|---|---|")
    for area in WORKER_AREAS:
        out.append(f"| {area} | {area_files.get(area, 0)} | "
                   f"{area_loc.get(area, 0):,} |")
    out.append("")
    if dropped:
        out.append(f"_Note: {len(dropped)} file(s) present in a sidecar or previous "
                   "map no longer exist on disk and were dropped from the table._")
        out.append("")
    if stale_sidecars:
        out.append(f"_Note: {len(stale_sidecars)} sidecar entr(ies) reference files "
                   "not on disk (kept in the yml, excluded from the table):_")
        for rel in stale_sidecars:
            out.append(f"  - {rel}")
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


def fmt_pct(num, den):
    return f"{(100.0 * num / den):.1f}%" if den else "0.0%"


def main():
    ap = argparse.ArgumentParser(description="Generate docs/parity/swift-port-map.md")
    ap.add_argument("--root", default=None,
                    help="Repo root (default: parent of the scripts/ dir holding this file)")
    args = ap.parse_args()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    root = args.root or os.path.dirname(script_dir)
    legacy_root = os.path.join(root, "clients", "legacy")
    sidecar_dir = os.path.join(root, "docs", "parity", "swift-port")
    out_path = os.path.join(root, "docs", "parity", "swift-port-map.md")

    files = collect_swift_files(legacy_root)
    claimed = load_sidecars(sidecar_dir)

    rows = {}
    stale_sidecars = sorted(rel for rel in claimed if rel not in files)
    for rel in files:
        if rel in claimed:
            area, entry = claimed[rel]
            dest = entry.get("destination") or heuristic_destination(rel)
            status = entry.get("status", "todo")
            checklist = entry.get("checklist", "")
            # checklist may be a list in the yml; render comma-separated
            if isinstance(checklist, list):
                checklist = ", ".join(str(c) for c in checklist)
            tests = tests_ported_count(entry)
            rows[rel] = (dest, status, checklist, tests)
        else:
            rows[rel] = (heuristic_destination(rel), "todo", "", 0)

    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write(render_map(files, rows, [], stale_sidecars))

    total_loc = sum(files.values())
    n_claimed = len(claimed) - len(stale_sidecars)
    print(f"swift files: {len(files)}, total LOC: {total_loc}, "
          f"sidecar-claimed rows: {n_claimed}, "
          f"stale sidecar entries: {len(stale_sidecars)}")
    for rel in stale_sidecars:
        print(f"  stale sidecar entry (file not on disk): {rel}")


if __name__ == "__main__":
    main()
