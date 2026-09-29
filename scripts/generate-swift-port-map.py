#!/usr/bin/env python3
"""Generate docs/parity/swift-port-map.md: one row per Swift file in the repo.

Walks ALL Swift files in the repo (excluding the gpuidart submodule, .git,
and the untracked port-shared/ duplicate), assigns an initial port
destination per file via filename heuristics, applies per-area sidecar
overrides from docs/parity/swift-port/<area>.yml, and writes a Markdown table
with Path | LOC | Destination | Status | Checklist rows | Tests ported.

Vendored code (clients/legacy/native/vendor/libghostty-spm) IS included,
marked as delete-with-app: it ships inside the legacy app and goes away
with it under the Swift-0% goal.

SIDECAR WORKFLOW (no more merge conflicts):
  - Workers NEVER hand-edit docs/parity/swift-port-map.md. It is generated.
  - Each worker area owns docs/parity/swift-port/<area>.yml. Edit ONLY your
    area's file. Two workers never touch the same file.
  - Sidecar fields per Swift file: destination, status, checklist, behaviours,
    tests, notes. The sidecar WINS over the heuristic for every field it sets.
  - Sidecar keys for files under clients/legacy/ may be written either
    repo-root-relative (clients/legacy/...) or legacy-relative (shared/...);
    both resolve to the same file. Keys for files outside clients/legacy/
    (generated/, crates/) must be repo-root-relative.
  - Re-run this script after editing your sidecar, then commit both the yml
    and the regenerated md:
        python3 scripts/generate-swift-port-map.py

RE-RUNNABLE / IDEMPOTENT: with no sidecar changes the output is
byte-identical. Path/LOC are always refreshed from disk. Files deleted from
the tree are dropped from the map (reported on stdout); sidecar entries for
missing files are reported as warnings but kept.

Duplicate Swift paths across sidecars are a hard ERROR (exit 1): each file
is claimed by exactly one area.

STRICT "ported" STANDARD: a row is `ported` only when EVERY Swift behaviour
(func, gesture, keybinding, state transition) maps to a Dart/Rust function
PLUS a test. List behaviours in the sidecar; `partial` means some behaviours
are ported. Zero tests means not ported.

LOC = total lines per file (including blanks and comments).
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

# Strict duplicate-key loader: a duplicate Swift path inside one sidecar file
# used to be silently swallowed (PyYAML keeps the last value), hiding rows
# from the generated map — e.g. the store Swift file keyed twice in
# macos-services.yml. Any duplicate key is now a hard error naming the
# file and key.

class _StrictLoader(yaml.SafeLoader):
    """SafeLoader that fails on duplicate mapping keys.

    A duplicate Swift path inside one sidecar file used to be silently
    swallowed (PyYAML keeps the last value), hiding rows from the generated
    map — e.g. the store Swift file keyed twice in macos-services.yml. Any
    duplicate key is now a hard error naming the file and key.
    """


def _construct_strict_mapping(loader, node, deep=False):
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=True)
        if key in mapping:
            raise yaml.YAMLError(f"duplicate key in sidecar: {key!r}")
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


_StrictLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
    _construct_strict_mapping,
)

# Repo-root-relative directories to walk for Swift files.
# clients/legacy holds the frozen legacy clients (native, iOS, app-kit,
# shared, dioxus bridges, dmg background, vendored libghostty-spm).
# generated/ holds the JSON runtime catalog (Swift output deleted under Swift-0%).
# crates/supercli-cli/tests holds the Swift test clients (pairclient,
# relayclient).
SWIFT_ROOTS = [
    "clients/legacy",
    "generated",
    "crates/supercli-cli/tests",
]

# Directory names (exact, case-sensitive) skipped during the walk.
EXCLUDE_DIRNAMES = (".git", "gpuidart", "port-shared")

# Path fragments (case-insensitive) that mark third-party code to exclude.
# NOTE: vendor/libghostty-spm is intentionally NOT excluded: it is tracked
# as delete-with-app rows so the headline counts every Swift file.
EXCLUDE_FRAGMENTS = ("third-party", "third_party")

# Baseline Swift LOC before any Swift-0% deletions. The deletion ledger
# (docs/parity/swift-deleted.md) records every deleted file; the map headline
# shows "Deleted: X of <baseline> baseline LOC (Y%)" so progress toward 0%
# is visible, not just what remains.
SWIFT_BASELINE_LOC = 167628


def deleted_loc_from_ledger(root):
    """Sum LOC of ledger rows with Status 'merged' in docs/parity/swift-deleted.md.

    The ledger table columns are: File | LOC | Deleting commit | Status |
    Rust/Dart target | Tests. Only rows whose Status cell is exactly 'merged'
    count — 'pending' rows are verified but not yet deleted on this branch.
    Returns 0 if the ledger is missing or unparseable.
    """
    ledger = os.path.join(root, "docs", "parity", "swift-deleted.md")
    total = 0
    try:
        with open(ledger, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line.startswith("|"):
                    continue
                cells = [c.strip() for c in line.strip("|").split("|")]
                if len(cells) < 6:
                    continue
                if cells[0].lower() == "file":
                    continue  # header row
                if set(cells[1]) <= set("- "):
                    continue  # separator row
                try:
                    loc = int(cells[1].replace(",", ""))
                except ValueError:
                    continue
                if cells[3].lower() == "merged":
                    total += loc
    except (FileNotFoundError, OSError):
        pass
    return total


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
D_DELETE_WITH_APP = "dropped: vendored libghostty-spm, deleted with app"

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
    """Initial destination guess from the repo-root-relative path."""
    # Vendored Ghostty SPM: delete-with-app, never ported.
    if "vendor/libghostty-spm" in rel_path.lower():
        return D_DELETE_WITH_APP
    # Swift test clients for the Rust CLI.
    if rel_path.startswith("crates/supercli-cli/tests/"):
        return D_RUST_LOGIC
    # Dioxus native-shell bridges.
    if rel_path.startswith("clients/legacy/dioxus/"):
        return D_RUST_BRIDGE
    # DMG background script.
    if rel_path == "clients/legacy/native/dmg-background.swift":
        return D_RUST_BRIDGE
    stem = os.path.splitext(os.path.basename(rel_path))[0]
    if rel_path.startswith("clients/legacy/shared/SupercliShared/"):
        return D_RUST_SHARED
    if rel_path.startswith("clients/legacy/app-kit/swift/"):
        return D_DART_APPKIT
    if rel_path.startswith("clients/legacy/ios/SupercliIOS/"):
        if contains_any(stem, IOS_UI_KW):
            return D_GAP_IOS_UI
        if contains_any(stem, IOS_LOGIC_KW):
            return D_RUST_IOS
        return D_TBD
    if rel_path.startswith("clients/legacy/native/SupercliNative/"):
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
    low = rel_path.lower()
    if "vendor/libghostty-spm" in low:
        return "terminal"  # Ghostty terminal vendored code
    if rel_path.startswith("clients/legacy/shared/SupercliShared/"):
        return "shared"
    if rel_path.startswith("clients/legacy/app-kit/swift/"):
        return "appkit"
    if rel_path.startswith("clients/legacy/ios/SupercliIOS/"):
        return "ios"
    if rel_path.startswith("clients/legacy/dioxus/"):
        return "macos-services"  # native shell bridges
    if rel_path == "clients/legacy/native/dmg-background.swift":
        return "macos-services"
    if rel_path.startswith("crates/supercli-cli/tests/"):
        return "remote"  # Swift test clients for pairing/relay
    if rel_path.startswith("clients/legacy/native/SupercliNative/"):
        stem = os.path.splitext(os.path.basename(rel_path))[0]
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


def collect_swift_files(root):
    """Return {repo_rel_path: loc} for all in-scope Swift files, sorted.

    Walks SWIFT_ROOTS from the repo root. Skips EXCLUDE_DIRNAMES
    (.git, gpuidart submodule, untracked port-shared/ duplicate) and
    EXCLUDE_FRAGMENTS (third-party code). Vendored libghostty-spm IS
    included (delete-with-app rows).
    """
    files = {}
    for swift_root in SWIFT_ROOTS:
        area_root = os.path.join(root, swift_root)
        if not os.path.isdir(area_root):
            print(f"WARNING: missing Swift root {area_root}", file=sys.stderr)
            continue
        for dirpath, dirnames, filenames in os.walk(area_root):
            # Prune excluded directories (in-place so os.walk skips them).
            dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRNAMES]
            for name in filenames:
                if not name.endswith(".swift"):
                    continue
                full = os.path.join(dirpath, name)
                rel = os.path.relpath(full, root).replace(os.sep, "/")
                lowered = rel.lower()
                if any(frag in lowered for frag in EXCLUDE_FRAGMENTS):
                    continue
                with open(full, "r", encoding="utf-8", errors="replace") as fh:
                    loc = sum(1 for _ in fh)
                files[rel] = loc
    return dict(sorted(files.items()))


def normalize_sidecar_key(key):
    """Normalize a sidecar key to a repo-root-relative path.

    Legacy sidecar keys are relative to clients/legacy/ (e.g.
    "shared/SupercliShared/Foo.swift"). New keys for files outside
    clients/legacy/ must already be repo-root-relative (e.g.
    "crates/supercli-cli/tests/pairclient/main.swift"). Both resolve to the same
    repo-root-relative path used as the map row key.
    """
    if key.startswith(("clients/", "generated/", "crates/")):
        return key
    return "clients/legacy/" + key


def load_sidecars(sidecar_dir):
    """Load all <area>.yml -> {repo_rel_path: entry}. Validates area names."""
    claimed = {}
    if not os.path.isdir(sidecar_dir):
        return claimed
    for fname in sorted(os.listdir(sidecar_dir)):
        if not fname.endswith(".yml"):
            continue
        area = fname[:-4]
        path = os.path.join(sidecar_dir, fname)
        with open(path, "r", encoding="utf-8") as fh:
            try:
                data = yaml.load(fh, Loader=_StrictLoader) or {}
            except yaml.YAMLError as exc:
                print(f"ERROR: {fname}: {exc}", file=sys.stderr)
                sys.exit(1)
        if data.get("area", area) != area:
            print(f"WARNING: {fname} declares area '{data.get('area')}', "
                  f"expected '{area}'", file=sys.stderr)
        for raw_rel, entry in (data.get("files") or {}).items():
            if not isinstance(entry, dict):
                print(f"WARNING: {fname}: entry for {raw_rel} is not a mapping; "
                      "skipped", file=sys.stderr)
                continue
            status = entry.get("status", "todo")
            if status not in STATUSES:
                print(f"WARNING: {fname}: {raw_rel} has unknown status "
                      f"'{status}'; expected one of {STATUSES}", file=sys.stderr)
            rel = normalize_sidecar_key(raw_rel)
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


def render_map(files, rows, dropped, stale_sidecars, deleted_loc=0):
    """files: {rel: loc}; rows: {rel: (dest, status, checklist, tests)}.

    deleted_loc: Swift LOC already deleted toward 0% (from the ledger).
    """
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
    out.append("One row per Swift file in the repo (gpuidart submodule excluded;")
    out.append("vendored libghostty-spm included as delete-with-app rows).")
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
    out.append(f"- Deleted: {deleted_loc:,} of {SWIFT_BASELINE_LOC:,} baseline LOC "
               f"({fmt_pct(deleted_loc, SWIFT_BASELINE_LOC)})")
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
    sidecar_dir = os.path.join(root, "docs", "parity", "swift-port")
    out_path = os.path.join(root, "docs", "parity", "swift-port-map.md")

    files = collect_swift_files(root)
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
    deleted_loc = deleted_loc_from_ledger(root)
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write(render_map(files, rows, [], stale_sidecars, deleted_loc))

    total_loc = sum(files.values())
    n_claimed = len(claimed) - len(stale_sidecars)
    print(f"swift files: {len(files)}, total LOC: {total_loc}, "
          f"sidecar-claimed rows: {n_claimed}, "
          f"stale sidecar entries: {len(stale_sidecars)}, "
          f"deleted: {deleted_loc:,} of {SWIFT_BASELINE_LOC:,} baseline LOC "
          f"({fmt_pct(deleted_loc, SWIFT_BASELINE_LOC)})")
    for rel in stale_sidecars:
        print(f"  stale sidecar entry (file not on disk): {rel}")


if __name__ == "__main__":
    main()
