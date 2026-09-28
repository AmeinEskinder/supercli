#!/usr/bin/env python3
"""Genuine sealed-pairing controller for the supercli Host.

Implements the phone half of the sealed /mobile/pair exchange, byte-compatible
with `supercli_client::pair` (crates/supercli-client/src/pairing.rs) and the
native MobilePairingStore:

- QR code text: `SUPERCLI:1:<host>:<port>:<MACID-UPPER>:<token>:<expiresSec>`
- Token: compared verbatim server-side (never case-folded)
- Envelope: {"v":1,"saltB64","sealedB64"} standard padded base64;
  sealed = nonce(12) || ciphertext || tag(16)
- Key: HKDF-SHA256(ikm=token UTF-8 text, salt=16 random,
  info="supercli-pairing-v1:phone-to-mac"), 32 bytes
- AAD: b"supercli-pairing-v1\\0phone-to-mac\\0<mac_id>\\0<endpoint>"
  (mac_id lowercased, endpoint byte-for-byte from the QR)

Reads the QR code from argv[1], POSTs the sealed envelope to
<endpoint>/pair, decrypts the sealed response (direction mac-to-phone),
and prints {"device_id","auth_token","endpoint"} as JSON.

This is the real authenticated pairing — not a seeded fixture.
"""
import base64
import json
import os
import sys
import urllib.request

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF


def decode_pairing_code(raw):
    parts = raw.strip().split(":")
    if len(parts) not in (7, 8):
        raise ValueError("bad QR field count")
    if parts[0].upper() != "SUPERCLI":
        raise ValueError("not a SUPERCLI code")
    host, port = parts[2], parts[3]
    mac_id = parts[4].lower()
    token = parts[5]
    if not host or not mac_id or not token:
        raise ValueError("empty QR field")
    endpoint = f"http://{host}:{port}/mobile"
    return endpoint, mac_id, token


def derive_key(token: str, salt: bytes, direction: str) -> bytes:
    info = f"supercli-pairing-v1:{direction}".encode()
    hkdf = HKDF(algorithm=hashes.SHA256(), length=32, salt=salt, info=info)
    return hkdf.derive(token.encode("utf-8"))


def associated_data(direction: str, mac_id: str, endpoint: str) -> bytes:
    return (
        b"supercli-pairing-v1\x00"
        + direction.encode()
        + b"\x00"
        + mac_id.encode()
        + b"\x00"
        + endpoint.encode()
    )


def seal(plaintext: bytes, token: str, mac_id: str, endpoint: str) -> dict:
    salt = os.urandom(16)
    key = derive_key(token, salt, "phone-to-mac")
    nonce = os.urandom(12)
    ct = AESGCM(key).encrypt(
        nonce, plaintext, associated_data("phone-to-mac", mac_id, endpoint)
    )
    return {
        "v": 1,
        "saltB64": base64.b64encode(salt).decode(),
        "sealedB64": base64.b64encode(nonce + ct).decode(),
    }


def open_envelope(envelope: dict, token: str, mac_id: str, endpoint: str) -> bytes:
    if envelope.get("v") != 1:
        raise ValueError("bad envelope version")
    salt = base64.b64decode(envelope["saltB64"])
    sealed = base64.b64decode(envelope["sealedB64"])
    if len(salt) != 16 or len(sealed) < 12 + 16:
        raise ValueError("bad envelope sizes")
    key = derive_key(token, salt, "mac-to-phone")
    nonce, ct = sealed[:12], sealed[12:]
    return AESGCM(key).decrypt(
        nonce, ct, associated_data("mac-to-phone", mac_id, endpoint)
    )


def main():
    if len(sys.argv) != 2:
        print("usage: pair_live_host.py '<SUPERCLI:1:... code>'", file=sys.stderr)
        sys.exit(2)
    endpoint, mac_id, token = decode_pairing_code(sys.argv[1])
    device_id = "wire-settings-shot"
    request_body = json.dumps(
        {
            "token": token,
            "device": {
                "id": device_id,
                "name": "wire-settings-shot",
                "platform": "test",
            },
        }
    ).encode()

    envelope = seal(request_body, token, mac_id, endpoint)
    url = endpoint.rstrip("/") + "/pair"
    req = urllib.request.Request(
        url,
        data=json.dumps(envelope).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        status, body = resp.status, resp.read()
    if not 200 <= status < 300:
        raise SystemExit(f"pairing HTTP {status}: {body[:200]}")

    plaintext = open_envelope(json.loads(body), token, mac_id, endpoint)
    paired = json.loads(plaintext)
    if paired.get("deviceID") != device_id:
        raise SystemExit("device id mismatch in pairing response")
    auth_token = paired.get("authToken")
    if not auth_token:
        raise SystemExit("no authToken in pairing response")
    print(
        json.dumps(
            {
                "device_id": paired["deviceID"],
                "auth_token": auth_token,
                "endpoint": paired.get("directEndpoint") or paired["endpoint"],
            }
        )
    )


if __name__ == "__main__":
    main()
