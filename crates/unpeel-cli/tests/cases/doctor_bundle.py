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
