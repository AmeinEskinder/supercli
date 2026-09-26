# Connectors — plugins for the Muse harness

**Status:** implemented (the whole track is done: signing, bundles, registry,
MCP-over-HTTP, OAuth2 dance + refresh, config forms, host consumption,
registrar sync, audit). This document is the reference for the manifest,
lifecycle, bundle/key formats, registry index, and security contract.

## What a connector is

A connector plugs an external service (email, calendar, issue tracker,
payments, …) into agent sessions as **tools**. To the agent it looks like
MCP tools; to the owner it looks like one connected account; to a developer
it looks like a manifest plus a small executable.

```
~/.supercli/connectors/
  gmail/
    connector.toml        # manifest (this spec)
    connector             # executable: the connector's MCP server
    config.json           # non-secret configuration (never secrets)
  linear/
    ...
```

Secrets live in the OS keychain (via the `keyring` crate, selected once for
the whole program), never in files.

## Manifest: `connector.toml` v1

```toml
[connector]
name = "gmail"
version = "1.2.0"
display_name = "Gmail"
description = "Read, search, draft and send mail."
kind = "mcp-stdio"            # mcp-stdio | mcp-http

[auth]
flow = "oauth2"               # oauth2 | api-key | none
scopes = ["https://www.googleapis.com/auth/gmail.modify"]
# The harness runs the OAuth dance once; the connector only ever sees
# the resulting token via the keychain handle below.

# OAuth2 providers (required when auth.flow = "oauth2"). client_id lives
# in the connector's config.json (non-secret); the dance is
# authorization-code + PKCE (S256) against a loopback redirect, and the
# stored TokenSet (access + refresh token, expiry, token_url) is
# refreshed automatically before it expires.
[oauth]
authorize_url = "https://accounts.google.com/o/oauth2/v2/auth"
token_url = "https://oauth2.googleapis.com/token"

[tools]
# Tools the connector offers. The harness exposes exactly these names to
# sessions — the connector cannot add tools at runtime.
provides = ["mail.search", "mail.read", "mail.draft", "mail.send"]

[policy]
# Default approval policy per tool when a session uses it. The owner can
# tighten per session; they can never loosen beyond what the manifest
# declares.
"mail.search" = "allow"
"mail.read"   = "allow"
"mail.draft"  = "ask"
"mail.send"   = "ask"

[config_schema]
# JSON Schema for the non-secret configuration the developer needs.
# Rendered as a form in the GUI / asked on the CLI at install time.
account_hint = { type = "string", title = "Account label" }
```

## Lifecycle

| Verb | What happens |
|---|---|
| `supercli connector discover` | Scan registrar sources for connector manifests (built-in dir, user dir, registries). |
| `supercli connector keygen [--key-id <id>]` | Generate an Ed25519 publisher keypair (secret stays `0600` in the keys dir, refuses to overwrite). |
| `supercli connector pack <name> [--out <dir>] [--key-id <id>]` | Build a signed `.supercli-connector` bundle: gzip tar of the connector dir (only `connector.toml`, `connector`, `config.json`, `README*`, `LICENSE*`, `icon.*` allowed) plus a detached Ed25519 `.sig` sidecar. |
| `supercli connector publish <name> --registry <dir> [--key-id <id>]` | Verify the signature, pin the publisher key on first publish (a later publish with a different key is refused), record the semver version, SHA-256 digest, and trusted-timestamp in the registry's `index.json`. |
| `supercli connector install <source> [key=value ...] [--form] [--require-signature]` | Install from a signed bundle (signature verified against the publisher key; wrong/missing key refused), from a registry (latest semver, digest re-checked), or from a directory (unsigned — reported honestly; `--require-signature` refuses it). `--form` renders `config_schema` interactively; `key=value` pairs and `config` are schema-validated and coerced into `config.json` (non-secrets only). |
| `supercli connector config <name> [key=value ...]` | Show or schema-validated update of `config.json`. |
| `supercli connector connect <name> [--token]` | Run the auth flow: `api-key` reads `--token` or prompts, `none` is a no-op, `oauth2` runs the authorization-code + PKCE (S256) browser dance against `[oauth]` endpoints with a loopback redirect and stores the TokenSet. Tokens go to the keychain under `connector:<name>`. |
| `supercli connector disconnect <name>` | Revocation is one verb: delete the keychain token and detach the connector from every session's `connectors.json` (reported as `detached_sessions`). |
| `supercli connector enable <name> --session <id> [--policy tool=ask ...]` | Attach the connector's tools to a session (writes `<session-dir>/connectors.json`, flock + atomic rename). Requires the connector to be connected when its auth flow needs a token. `--policy` may only tighten the manifest ceiling. `disable <name> --session <id>` detaches (idempotent). The Host consumes the record: the session's MCP server advertises attached non-Deny tools in `tools/list` and dispatches `tools/call` through a per-session link (stdio process with `SUPERCLI_CONNECTOR_TOKEN` injected, or MCP-over-HTTP via `mcp_url` in `config.json`), enforces Allow/Ask/Deny (Ask prompts via `/mcp/approve-connector`, grants persist per session+tool in `app-state.json`), re-reads the record on every call so detach takes effect without restart, fails closed on corrupt records or missing/revoked tokens, and appends an audit entry per call to `<session-dir>/connectors-audit.jsonl`. OAuth2 access tokens are refreshed automatically before expiry and rotated back into the keychain. |
| `supercli connector doctor` | Every connector: manifest valid, executable present (stdio) or `mcp_url` reachable (http), token present when auth needs one (OAuth2 tokens refreshed when expiring), tools responding. `auth.flow = "none"` never needs a token. |
| `supercli connector run <name> <tool> [key=value ...]` | Invoke one tool through the full path (policy → keychain → MCP). Non-interactive: Allow tools only. Works for stdio and HTTP connectors; OAuth2 tokens refresh first. |
| `supercli connector audit --session <id> [--tool <t>]` | Query the session's `connectors-audit.jsonl` (filter by tool, JSON output). |
| `supercli connector sync [--registry <dir>]` / `supercli registrar sync [--registry <dir>]` | Install/update every registry connector to its latest version (digest-checked fetch, signature verified against the pinned key before anything is written), then verify everything installed: transport present, token present when auth requires one. Updates are staged beside the install dir and swapped in atomically — a failed update leaves the old version untouched, and the user's local `config.json` is preserved over the bundle's. With no `--registry` it only verifies. |

`SUPERCLI_CONNECTORS_DIR` overrides the scan roots; `SUPERCLI_CONNECTORS_INSTALL_DIR`
overrides the install dir (both exist for tests/dev). `SUPERCLI_CONNECTORS_KEYCHAIN=memory`
forces the in-process token store (tests); otherwise the OS keychain is used when it
answers, with a process-lifetime in-memory fallback and a stderr warning. Note: on
headless Linux the keyring backend may accept writes without persisting them across
sessions — the same limitation the Controller pairing store already has.

## Developer story

```sh
supercli connector keygen --key-id me        # Ed25519 publisher keypair
supercli connector pack myservice            # signed .supercli-connector bundle
supercli connector publish myservice --registry my-registry
supercli registrar sync --registry my-registry   # install/update + verify
```

A connector executable is **just an MCP server** on stdio (or HTTP). No SDK
required: any language that speaks MCP works. The harness:

1. spawns it per session that enabled the connector,
2. injects the keychain token as an env var (`SUPERCLI_CONNECTOR_TOKEN`),
3. filters its tool list to `tools.provides` (a connector that starts
   offering new tools fails closed),
4. wraps every call in the session's approval policy.

## Security contract (non-negotiable)

- **No new sandbox promises.** Same cooperative model as the Host: the
  approval policy is enforced on the harness's tool path, and a session
  that reaches the network directly bypasses it. Connectors never claim
  containment.
- **Least privilege by construction.** `tools.provides` is a closed list;
  `policy` defaults are ceilings, not floors; OAuth scopes are declared
  in the manifest and shown to the owner before `connect`.
- **Secrets never touch disk in plaintext.** Keychain only. `config.json`
  holds non-secrets; a connector that writes a secret to disk fails
  `doctor` and is quarantined.
- **Auditability.** Every connector tool call is appended to the session
  transcript with the connector name, so the owner can see exactly what
  left the machine.
- **Revocation is one verb.** `supercli connector disconnect gmail` deletes
  the keychain entry and detaches the tools from all sessions.

## Relationship to other tracks

- **Registrar (D):** connectors are a `kind = "connector"` registrar item;
  `registrar.toml` pins versions, `registrar.lock` records them.
- **Mind (A):** the mind's headless jobs use connectors through the same
  tool path (e.g. the morning briefing reads mail via `mail.search`).
  The mind never holds tokens — it calls tools, the harness injects them.
- **Chat (C) / Dioxus (B):** connector install/connect/enable/doctor are
  verbs first; the GUI renders them as cards. The OAuth dance opens the
  system browser from any client.

## Bundle, key, and registry formats

- **Keys:** Ed25519. The secret file is `0600` Unix mode; both the secret
  and public files carry a versioned header so formats can evolve.
  Key rotation policy: `publish` pins the publisher key on first publish
  and refuses any later publish signed by a different key — rotating
  means publishing under a new key id and re-pinning the registry entry.
- **Bundles:** `.supercli-connector` is a gzip tar of the connector dir
  (allowed files only: `connector.toml`, `connector`, `config.json`,
  `README*`, `LICENSE*`, `icon.*`; extraction is path-traversal safe)
  with a detached Ed25519 `.sig` sidecar.
- **Registry:** `index.json` per registry dir: name → versions →
  `{ version, sha256, pubkey, published_at }`. Semver resolution picks
  the latest; the digest is re-verified on every install.

## Open decisions

- [x] Manifest signing: Ed25519 with versioned key files (decided; custom
  format, not minisign/sigstore).
- [x] Registry protocol: a registry is a directory with `index.json`
  plus publisher key pinning (decided).
- [ ] Built-in connectors v1 set: propose `gmail`, `google-calendar`, `github`.
