//! `supercli connector` — Host-side connector (plugin) management.
//!
//! Implements the connector lifecycle from `docs/connectors.md`:
//! `discover` scans the registrar sources, `install` copies a connector
//! into the user's install dir, `connect` runs the auth flow once and
//! stores the token in the keychain, `doctor` validates every connector
//! end to end (manifest, executable, token, tools responding),
//! `disconnect` revokes the token and detaches the connector from every
//! session, and `run` invokes one Allow tool through the full path
//! (discovery → policy → keychain → connector link). `enable` /
//! `disable` attach and detach a connector's tools to one session
//! (`<session-dir>/connectors.json`); the session's MCP server consumes
//! the record when it lists and dispatches tools (opening a link per
//! session — stdio process or MCP-over-HTTP — injecting the keychain
//! token, filtering tools to `tools.provides`, enforcing the effective
//! approval policy, refreshing OAuth2 tokens before they expire).
//! `keygen`/`pack`/`publish` are the publisher side: Ed25519 keypairs,
//! signed `.supercli-connector` bundles, and a local registry index that
//! pins each publisher's key. `config` renders the manifest's
//! `config_schema` as an interactive form (or validates key=value pairs)
//! into the installed connector's `config.json`. `audit` queries the
//! per-session connector-call audit log. `sync` is the connector slice of
//! `registrar sync`: discover → resolve → install → verify → report.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};
use supercli_client::CredentialStore;
use supercli_connector::{
    auth_flow_from_str, bundle_manifest, config_string, connector_kind_from_str, default_roots,
    delete_connector_token, disable_attachment, discover, effective_policy, enable_attachment,
    fetch, keygen, load_connector_token, load_public_key, mcp_url_from_config, merge_config,
    open_connector_store, pack, parse_manifest, parse_public_key, public_key_base64, publish,
    read_index, read_signature, resolve, resolve_connector_token, run_form, run_oauth_dance,
    scaffold, store_connector_token, unpack_verified, validate_pairs, ApprovalPolicy, AuthFlow,
    ConnectorKind, ConnectorLink, ConnectorManifest, DiscoveredConnector, ScaffoldError,
    ScaffoldOptions,
};

pub const HELP: &str = "\
supercli connector — Host-side connectors (plugins)

  supercli connector new <name> [--dir <path>] [--kind mcp-stdio|mcp-http]
                                  [--auth none|api-key|oauth2]
                                  [--tool <name>]... [--scope <scope>]...
                                  [--display-name <text>]
                                  [--description <text>] [--json]
                                  scaffold a new connector directory
                                  (<dir>/<name>/connector.toml plus a stub
                                  `connector` MCP executable, or config.json
                                  for mcp-http); refuses to overwrite
  supercli connector discover [--json]
                                  scan for installed connector.toml manifests
  supercli connector install <name-or-path> [key=value ...] [--json]
                                  copy a connector into the user's install dir
                                  (~/.supercli/connectors/<name>/); <name> is a
                                  discovered connector, <path> a directory
                                  holding connector.toml, or a
                                  .supercli-connector bundle file. key=value
                                  pairs are written to config.json
                                  (non-secrets only, validated against the
                                  manifest's config_schema when present)
  supercli connector install --registry <dir> <name>[@version] [--json]
                                  install the latest (or pinned) version from
                                  a registry; the bundle signature is always
                                  verified against the registry's pinned
                                  publisher key
  supercli connector install ... [--form]
                                  render the manifest's config_schema as an
                                  interactive questionnaire instead of
                                  key=value pairs
  supercli connector install ... [--require-signature] [--pubkey <file>]
                                  refuse installs that cannot be signature-
                                  verified; --pubkey supplies the trusted key
                                  for a bundle file
  supercli connector config <name> [--form] [key=value ...] [--json]
                                  show (or update) the installed connector's
                                  config.json; --form re-renders the schema
                                  questionnaire, key=value pairs are
                                  validated against it
  supercli connector keygen [--key-id <id>] [--json]
                                  generate an Ed25519 publisher keypair
                                  (~/.supercli/connector-keys/<id>.key/.pub)
  supercli connector pack <name> [--out <dir>] [--key-id <id>] [--json]
                                  build a signed .supercli-connector bundle
  supercli connector publish <name> --registry <dir> [--key-id <id>] [--json]
                                  pack and publish to a registry (first
                                  publish pins the publisher key)
  supercli connector sync [--registry <dir>] [--json]
                                  discover → resolve → install → verify →
                                  report; with --registry, install/update
                                  every registry connector to its latest
                                  version
  supercli connector audit --session <id> [--tool <name>] [--limit <n>] [--json]
                                  query the session's connector-call audit log
  supercli connector connect <name> [--token <value>] [--json]
                                  run the auth flow once: API-key prompt (or
                                  --token) or the OAuth2 browser dance; the
                                  token is stored in the keychain under
                                  supercli/connector/<name> (omit --token to be
                                  prompted)
  supercli connector disconnect <name> [--json]
                                  revoke: delete the keychain token (one verb)
                                  and detach the connector from every session
  supercli connector enable <name> --session <id> [--policy tool=ask ...] [--json]
                                  attach the connector's tools to a session
                                  (MCP registration); manifest policy is the
                                  ceiling, --policy can only tighten it
  supercli connector disable <name> --session <id> [--json]
                                  detach the connector from a session
  supercli connector doctor [--json]
                                  check every connector: manifest valid,
                                  transport up, token present (OAuth2 tokens
                                  are refreshed when expiring), tools
                                  responding (exit 1 on any failure)
  supercli connector run <name> <tool> [key=value ...] [--json]
                                  invoke one Allow tool end to end
                                  (discovery → policy → keychain → connector
                                  link)

Connectors live in ~/.supercli/connectors/<name>/ (connector.toml plus the
`connector` MCP executable for mcp-stdio, or config.json's `mcp_url` for
mcp-http). SUPERCLI_CONNECTORS_DIR overrides the scan roots;
SUPERCLI_CONNECTORS_INSTALL_DIR overrides the install dir (both for tests
and dev).

`run` is non-interactive: it only invokes tools whose effective approval
policy is Allow. Anything stricter refuses with the policy named — attach
it to a session (or loosen the manifest) instead.

`install` from a directory trusts the directory; signature verification
happens for registry installs and bundle files. A bundle's `.sig` sidecar
is verified against the registry's pinned publisher key (registry) or a
key you supply with --pubkey (bundle file).";

const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `connect` waits for the user to finish the OAuth2 browser
/// dance before giving up.
const DANCE_TIMEOUT: Duration = Duration::from_secs(300);

/// Scan roots: `SUPERCLI_CONNECTORS_DIR` (colon-separated, for tests and dev)
/// wins over the default registrar sources.
fn roots() -> Vec<PathBuf> {
    let mut scan = if let Some(dirs) = std::env::var_os("SUPERCLI_CONNECTORS_DIR") {
        let roots: Vec<PathBuf> = std::env::split_paths(&dirs).collect();
        if !roots.is_empty() {
            roots
        } else {
            default_roots()
        }
    } else {
        default_roots()
    };
    // The install dir is always scanned: a connector the user installed
    // must stay discoverable even when the scan roots are overridden
    // for tests/dev.
    let install = install_root();
    if !scan.iter().any(|r| r == &install) {
        scan.push(install);
    }
    scan
}

/// Install target: `SUPERCLI_CONNECTORS_INSTALL_DIR` (for tests and dev)
/// wins over `~/.supercli/connectors`.
fn install_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("SUPERCLI_CONNECTORS_INSTALL_DIR") {
        let path = PathBuf::from(dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".supercli").join("connectors"),
        None => PathBuf::from(".supercli-connectors"),
    }
}

/// The connector token store, with the keychain-unavailable fallback.
/// Emits the fallback notice to stderr when one applies.
fn connector_store() -> Arc<dyn CredentialStore> {
    let (store, notice) = open_connector_store();
    if let Some(notice) = notice {
        eprintln!("warning: {notice}");
    }
    store
}

fn find_connector(name: &str) -> Result<DiscoveredConnector, String> {
    let roots = roots();
    let (found, _) = discover(&roots.iter().map(|p| p.as_path()).collect::<Vec<_>>());
    found
        .into_iter()
        .find(|c| c.manifest.name == name)
        .ok_or_else(|| format!("no connector named {name:?} (see `supercli connector discover`)"))
}

fn executable_for(connector: &DiscoveredConnector) -> PathBuf {
    connector.dir.join("connector")
}

fn tool_policy(connector: &DiscoveredConnector, tool: &str) -> ApprovalPolicy {
    effective_policy(&connector.manifest, &HashMap::new(), tool)
}

/// The first CLI flag not in `allowed` (`--json` is always allowed), if any.
fn unknown_flag<'a>(args: &'a [String], allowed: &[&str]) -> Option<&'a str> {
    args.iter().find_map(|arg| {
        if !arg.starts_with("--") || arg == "--json" || allowed.contains(&arg.as_str()) {
            None
        } else {
            Some(arg.as_str())
        }
    })
}

fn flag_error(flag: &str) -> i32 {
    eprintln!("unknown connector option {flag:?}\n\n{HELP}");
    1
}

pub fn run(args: &[String]) -> i32 {
    let json = args.iter().any(|arg| arg == "--json");
    // `connect` takes `--token <value>` and `enable`/`disable` take
    // `--session <id>`, so they parse the raw args — the positional split
    // below would swallow the flag values.
    if args.first().is_some_and(|a| a == "connect") {
        return match args.get(1) {
            Some(name) => match connect_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok(token) => connect_cmd(name, token, json),
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args
        .first()
        .is_some_and(|a| a == "enable" || a == "disable")
    {
        let verb = args[0].as_str();
        return match args.get(1) {
            Some(name) => match session_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok((session, policies)) => {
                    if verb == "enable" {
                        enable_cmd(name, &session, &policies, json)
                    } else {
                        disable_cmd(name, &session, json)
                    }
                }
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args.first().is_some_and(|a| a == "new") {
        return match args.get(1) {
            Some(name) => match new_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok(opts) => new_cmd(name, opts, json),
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args.first().is_some_and(|a| a == "keygen") {
        return match keygen_flags(&args[1..]) {
            Err(e) => {
                eprintln!("{e}\n\n{HELP}");
                1
            }
            Ok(key_id) => keygen_cmd(&key_id, json),
        };
    }
    if args.first().is_some_and(|a| a == "pack") {
        return match args.get(1) {
            Some(name) => match pack_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok((out, key_id)) => pack_cmd(name, out.as_deref(), &key_id, json),
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args.first().is_some_and(|a| a == "publish") {
        return match args.get(1) {
            Some(name) => match publish_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok((registry, key_id)) => publish_cmd(name, &registry, &key_id, json),
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args.first().is_some_and(|a| a == "config") {
        return match args.get(1) {
            Some(name) => match config_flags(&args[2..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok((form, pairs)) => config_cmd(name, form, &pairs, json),
            },
            None => {
                eprintln!("{HELP}");
                1
            }
        };
    }
    if args.first().is_some_and(|a| a == "audit") {
        return match audit_flags(&args[1..]) {
            Err(e) => {
                eprintln!("{e}\n\n{HELP}");
                1
            }
            Ok(opts) => audit_cmd(opts, json),
        };
    }
    if args.first().is_some_and(|a| a == "sync") {
        return match sync_flags(&args[1..]) {
            Err(e) => {
                eprintln!("{e}\n\n{HELP}");
                1
            }
            Ok(registry) => sync_cmd(registry.as_deref(), json),
        };
    }
    let positional: Vec<&str> = args
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(String::as_str)
        .collect();
    match positional.as_slice() {
        [] | ["help"] => {
            println!("{HELP}");
            0
        }
        ["discover"] => match unknown_flag(args, &[]) {
            Some(flag) => flag_error(flag),
            None => discover_cmd(json),
        },
        ["install", ..] => match unknown_flag(
            args,
            &[
                "--registry",
                "--form",
                "--require-signature",
                "--pubkey",
                "--key-id",
            ],
        ) {
            Some(flag) => flag_error(flag),
            None => match install_flags(&args[1..]) {
                Err(e) => {
                    eprintln!("{e}\n\n{HELP}");
                    1
                }
                Ok(opts) => install_cmd(&opts, json),
            },
        },
        ["disconnect", name] => match unknown_flag(args, &[]) {
            Some(flag) => flag_error(flag),
            None => disconnect_cmd(name, json),
        },
        ["doctor"] => match unknown_flag(args, &[]) {
            Some(flag) => flag_error(flag),
            None => doctor_cmd(json),
        },
        ["run", name, tool, rest @ ..] => match unknown_flag(args, &[]) {
            Some(flag) => flag_error(flag),
            None => run_cmd(name, tool, rest, json),
        },
        _ => {
            eprintln!("{HELP}");
            1
        }
    }
}

/// Parse `connect`'s raw tail args: only `--token <value>` (and `--json`,
/// handled globally) is accepted.
fn connect_flags(tail: &[String]) -> Result<Option<String>, String> {
    let mut token: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--json" => {}
            "--token" => {
                let value = tail
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| "expected a value after --token".to_string())?;
                token = Some(value.to_string());
                i += 1;
            }
            other => return Err(format!("unknown connector option {other:?}")),
        }
        i += 1;
    }
    Ok(token)
}

/// Parse `enable`/`disable`'s raw tail args: `--session <id>` (required)
/// and repeatable `--policy <tool>=<allow|ask|deny>` (and `--json`,
/// handled globally).
fn session_flags(tail: &[String]) -> Result<(String, HashMap<String, ApprovalPolicy>), String> {
    let mut session: Option<String> = None;
    let mut policies: HashMap<String, ApprovalPolicy> = HashMap::new();
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--json" => {}
            "--session" => {
                let value = tail
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| "expected a session id after --session".to_string())?;
                session = Some(value.to_string());
                i += 1;
            }
            "--policy" => {
                let value = tail
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| "expected tool=policy after --policy".to_string())?;
                let (tool, policy) = value
                    .split_once('=')
                    .ok_or_else(|| format!("expected tool=policy, got {value:?}"))?;
                let policy: ApprovalPolicy = policy
                    .parse()
                    .map_err(|_| format!("unknown policy {policy:?}: allow | ask | deny"))?;
                policies.insert(tool.to_string(), policy);
                i += 1;
            }
            other => return Err(format!("unknown connector option {other:?}")),
        }
        i += 1;
    }
    let session = session.ok_or_else(|| "enable/disable require --session <id>".to_string())?;
    Ok((session, policies))
}

struct NewOptions {
    dir: Option<PathBuf>,
    kind: Option<ConnectorKind>,
    auth: Option<AuthFlow>,
    tools: Vec<String>,
    scopes: Vec<String>,
    display_name: Option<String>,
    description: Option<String>,
}

/// Parse `new`'s raw tail args: `--dir`, `--kind`, `--auth`,
/// `--display-name`, `--description` take one value; `--tool` and
/// `--scope` are repeatable; `--json` is handled globally.
fn new_flags(tail: &[String]) -> Result<NewOptions, String> {
    let mut opts = NewOptions {
        dir: None,
        kind: None,
        auth: None,
        tools: Vec::new(),
        scopes: Vec::new(),
        display_name: None,
        description: None,
    };
    let single = |slot: &mut Option<String>,
                  tail: &[String],
                  i: &mut usize,
                  flag: &str|
     -> Result<(), String> {
        let value = tail
            .get(*i + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("expected a value after {flag}"))?;
        *slot = Some(value.to_string());
        *i += 1;
        Ok(())
    };
    let mut dir: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut auth: Option<String> = None;
    let mut display_name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--json" => {}
            "--dir" => single(&mut dir, tail, &mut i, "--dir")?,
            "--kind" => single(&mut kind, tail, &mut i, "--kind")?,
            "--auth" => single(&mut auth, tail, &mut i, "--auth")?,
            "--display-name" => single(&mut display_name, tail, &mut i, "--display-name")?,
            "--description" => single(&mut description, tail, &mut i, "--description")?,
            "--tool" => {
                let value = tail
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| "expected a tool name after --tool".to_string())?;
                opts.tools.push(value.to_string());
                i += 1;
            }
            "--scope" => {
                let value = tail
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| "expected a scope after --scope".to_string())?;
                opts.scopes.push(value.to_string());
                i += 1;
            }
            other => return Err(format!("unknown connector option {other:?}")),
        }
        i += 1;
    }
    if let Some(dir) = dir {
        opts.dir = Some(PathBuf::from(dir));
    }
    if let Some(kind) = kind {
        opts.kind =
            Some(connector_kind_from_str(&kind).ok_or_else(|| {
                format!("unknown --kind {kind:?}: mcp-stdio | mcp-http | builtin")
            })?);
    }
    if let Some(auth) = auth {
        opts.auth = Some(
            auth_flow_from_str(&auth)
                .ok_or_else(|| format!("unknown --auth {auth:?}: none | api-key | oauth2"))?,
        );
    }
    opts.display_name = display_name;
    opts.description = description;
    Ok(opts)
}

/// Parse a single-value flag `--flag <value>` shared by the tail parsers.
fn take_value(tail: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    let value = tail
        .get(*i + 1)
        .filter(|v| !v.starts_with("--"))
        .ok_or_else(|| format!("expected a value after {flag}"))?;
    *i += 1;
    Ok(value.to_string())
}

/// Parse the raw tail args of a verb whose flags all take values or are
/// plain booleans, rejecting anything unexpected.
fn tail_flags(
    tail: &[String],
    verb: &str,
    valued: &[&str],
    bools: &[&str],
) -> Result<(HashMap<String, String>, Vec<String>), String> {
    let mut values = HashMap::new();
    let mut flags = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let word = tail[i].as_str();
        if word == "--json" {
            i += 1;
            continue;
        }
        if valued.contains(&word) {
            let value = take_value(tail, &mut i, word)?;
            values.insert(word.to_string(), value);
        } else if bools.contains(&word) {
            flags.push(word.to_string());
        } else {
            return Err(format!("unknown {verb} option {word:?}"));
        }
        i += 1;
    }
    Ok((values, flags))
}

/// `keygen --key-id <id>` (default "default").
fn keygen_flags(tail: &[String]) -> Result<String, String> {
    let (values, _) = tail_flags(tail, "keygen", &["--key-id"], &[])?;
    Ok(values
        .get("--key-id")
        .cloned()
        .unwrap_or_else(|| "default".to_string()))
}

/// `pack <name> --out <dir> --key-id <id>`.
fn pack_flags(tail: &[String]) -> Result<(Option<String>, String), String> {
    let (values, _) = tail_flags(tail, "pack", &["--out", "--key-id"], &[])?;
    Ok((
        values.get("--out").cloned(),
        values
            .get("--key-id")
            .cloned()
            .unwrap_or_else(|| "default".to_string()),
    ))
}

/// `publish <name> --registry <dir> --key-id <id>`.
fn publish_flags(tail: &[String]) -> Result<(String, String), String> {
    let (values, _) = tail_flags(tail, "publish", &["--registry", "--key-id"], &[])?;
    let registry = values
        .get("--registry")
        .cloned()
        .ok_or_else(|| "publish needs --registry <dir>".to_string())?;
    Ok((
        registry,
        values
            .get("--key-id")
            .cloned()
            .unwrap_or_else(|| "default".to_string()),
    ))
}

/// Install options parsed from the raw tail: the positional source and
/// key=value words, plus the install flags.
struct InstallOptions {
    source: String,
    config_pairs: Vec<String>,
    registry: Option<String>,
    form: bool,
    require_signature: bool,
    pubkey: Option<String>,
    key_id: Option<String>,
}

impl InstallOptions {
    fn key_id(&self) -> &str {
        self.key_id.as_deref().unwrap_or("default")
    }
}

fn install_flags(tail: &[String]) -> Result<InstallOptions, String> {
    let mut opts = InstallOptions {
        source: String::new(),
        config_pairs: Vec::new(),
        registry: None,
        form: false,
        require_signature: false,
        pubkey: None,
        key_id: None,
    };
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--json" => {}
            "--form" => opts.form = true,
            "--require-signature" => opts.require_signature = true,
            "--registry" => opts.registry = Some(take_value(tail, &mut i, "--registry")?),
            "--pubkey" => opts.pubkey = Some(take_value(tail, &mut i, "--pubkey")?),
            "--key-id" => opts.key_id = Some(take_value(tail, &mut i, "--key-id")?),
            word if word.starts_with("--") => {
                return Err(format!("unknown install option {word:?}"));
            }
            word => {
                if opts.source.is_empty() {
                    opts.source = word.to_string();
                } else {
                    opts.config_pairs.push(word.to_string());
                }
            }
        }
        i += 1;
    }
    if opts.source.is_empty() {
        return Err(
            "install needs a source: a name, a directory, a bundle, or --registry <dir> <name>"
                .to_string(),
        );
    }
    if opts.form && !opts.config_pairs.is_empty() {
        return Err(
            "install --form takes no key=value pairs — the form asks for everything".to_string(),
        );
    }
    Ok(opts)
}

/// `config <name> --form [key=value ...]`.
fn config_flags(tail: &[String]) -> Result<(bool, Vec<String>), String> {
    let mut form = false;
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        match tail[i].as_str() {
            "--json" => {}
            "--form" => form = true,
            word if word.starts_with("--") => {
                return Err(format!("unknown config option {word:?}"));
            }
            word => pairs.push(word.to_string()),
        }
        i += 1;
    }
    Ok((form, pairs))
}

struct AuditOptions {
    session: String,
    tool: Option<String>,
    limit: usize,
}

/// `audit --session <id> [--tool <name>] [--limit <n>]`.
fn audit_flags(tail: &[String]) -> Result<AuditOptions, String> {
    let (values, _) = tail_flags(tail, "audit", &["--session", "--tool", "--limit"], &[])?;
    let session = values
        .get("--session")
        .cloned()
        .ok_or_else(|| "audit needs --session <id>".to_string())?;
    let tool = values.get("--tool").cloned();
    let limit = match values.get("--limit") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| format!("bad --limit {raw:?}"))?,
        None => 50,
    };
    Ok(AuditOptions {
        session,
        tool,
        limit,
    })
}

/// `sync --registry <dir>`.
fn sync_flags(tail: &[String]) -> Result<Option<String>, String> {
    let (values, _) = tail_flags(tail, "sync", &["--registry"], &[])?;
    Ok(values.get("--registry").cloned())
}

fn new_cmd(name: &str, opts: NewOptions, json: bool) -> i32 {
    // The scaffold validates the name, kind, auth, and tool names through
    // the real manifest parser before anything touches the filesystem.
    let scaffolded = match scaffold(
        name,
        ScaffoldOptions {
            display_name: opts.display_name,
            description: opts.description,
            kind: opts.kind,
            auth_flow: opts.auth,
            auth_scopes: opts.scopes,
            tools: opts.tools,
        },
    ) {
        Ok(s) => s,
        Err(ScaffoldError::Builtin) => {
            eprintln!("cannot scaffold {name:?}: builtin connectors live inside the harness");
            return 1;
        }
        Err(e) => {
            eprintln!("cannot scaffold {name:?}: {e}");
            return 1;
        }
    };
    // Manifest names are [a-z0-9-]+ (enforced by the parser above), so the
    // join cannot escape the parent directory.
    let dest = opts.dir.unwrap_or_else(|| PathBuf::from(".")).join(name);
    if dest.exists() {
        eprintln!(
            "{} already exists — refusing to overwrite; remove it first or pick another name",
            dest.display()
        );
        return 1;
    }
    let write_file = |path: &Path, contents: &str| -> Result<(), String> {
        std::fs::write(path, contents)
            .map_err(|e| format!("could not write {}: {e}", path.display()))
    };
    if let Err(e) = std::fs::create_dir_all(&dest)
        .map_err(|e| format!("could not create {}: {e}", dest.display()))
        .and_then(|()| write_file(&dest.join("connector.toml"), &scaffolded.manifest_toml))
        .and_then(|()| {
            if let Some(stub) = &scaffolded.stub_source {
                let exe = dest.join("connector");
                write_file(&exe, stub)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
                        .map_err(|e| format!("could not chmod {}: {e}", exe.display()))?;
                }
            }
            if let Some(config) = &scaffolded.config_json {
                write_file(&dest.join("config.json"), config)?;
            }
            Ok(())
        })
    {
        eprintln!("{e}");
        return 1;
    }
    if json {
        let mut files = vec!["connector.toml"];
        if scaffolded.stub_source.is_some() {
            files.push("connector");
        }
        if scaffolded.config_json.is_some() {
            files.push("config.json");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": name,
                "dir": dest,
                "kind": format!("{:?}", scaffolded.manifest.kind),
                "files": files,
            }))
            .unwrap()
        );
    } else {
        println!("scaffolded {name} → {}", dest.display());
        println!(
            "next: `supercli connector install {}` then `supercli connector doctor`",
            dest.display()
        );
    }
    0
}

fn discover_cmd(json: bool) -> i32 {
    let roots = roots();
    let (found, errors) = discover(&roots.iter().map(|p| p.as_path()).collect::<Vec<_>>());
    if json {
        let out = json!({
            "connectors": found.iter().map(|c| json!({
                "name": c.manifest.name,
                "version": c.manifest.version.to_string(),
                "kind": format!("{:?}", c.manifest.kind),
                "auth": format!("{:?}", c.manifest.auth_flow),
                "tools": c.manifest.provides,
                "dir": c.dir,
            })).collect::<Vec<_>>(),
            "errors": errors.iter().map(|e| json!({
                "dir": e.dir,
                "error": e.error.to_string(),
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return 0;
    }
    if found.is_empty() {
        println!("No connectors installed.");
    }
    for c in &found {
        println!(
            "{} {} ({:?}, auth {:?}) — {} tool{}",
            c.manifest.name,
            c.manifest.version,
            c.manifest.kind,
            c.manifest.auth_flow,
            c.manifest.provides.len(),
            if c.manifest.provides.len() == 1 {
                ""
            } else {
                "s"
            },
        );
    }
    for e in &errors {
        eprintln!("warning: {}: {}", e.dir.display(), e.error);
    }
    0
}

struct DoctorReport {
    name: String,
    kind: String,
    transport_ok: bool,
    token_ok: bool,
    token_required: bool,
    refreshed: bool,
    tools_ok: bool,
    tool_count: usize,
    detail: String,
}

fn doctor_one(store: &dyn CredentialStore, connector: &DiscoveredConnector) -> DoctorReport {
    let name = connector.manifest.name.clone();
    // Transport: stdio needs the `connector` executable, mcp-http needs
    // an `mcp_url` in config.json.
    let (transport_ok, transport_detail) = match &connector.manifest.kind {
        ConnectorKind::McpStdio => {
            let exe = executable_for(connector);
            if exe.is_file() {
                (true, String::new())
            } else {
                (false, format!("missing executable {}", exe.display()))
            }
        }
        ConnectorKind::McpHttp => match mcp_url_from_config(&connector.dir) {
            Some(url) if !url.trim().is_empty() => (true, String::new()),
            _ => (
                false,
                "config.json has no mcp_url (mcp-http connectors need one)".to_string(),
            ),
        },
        ConnectorKind::Builtin => (
            false,
            "builtin connectors have no external transport to probe".to_string(),
        ),
    };
    // Token semantics: `auth.flow = "none"` connectors need no token and
    // are never failed for lacking one. Token-requiring connectors that
    // were never connected fail with the fix spelled out. OAuth2 tokens
    // are refreshed through the shared resolver before they expire.
    let token_required = connector.manifest.auth_flow != AuthFlow::None;
    let before = load_connector_token(store, &name).unwrap_or(None);
    let resolved = if token_required {
        resolve_connector_token(
            &connector.manifest,
            &connector.dir,
            store,
            &name,
            SPAWN_TIMEOUT,
        )
    } else {
        Ok(String::new())
    };
    let (token_value, token_ok, detail) = match resolved {
        Ok(token) => (token, true, String::new()),
        Err(e) => (
            String::new(),
            false,
            if matches!(e, supercli_connector::OAuthError::NotConnected) {
                format!("not connected — run `supercli connector connect {name}` first")
            } else {
                format!("token unavailable: {e}")
            },
        ),
    };
    let refreshed = load_connector_token(store, &name).unwrap_or(None) != before;
    let base = DoctorReport {
        name: name.clone(),
        kind: format!("{:?}", connector.manifest.kind),
        transport_ok,
        token_ok,
        token_required,
        refreshed,
        tools_ok: false,
        tool_count: 0,
        detail,
    };
    if !transport_ok {
        return DoctorReport {
            detail: transport_detail,
            ..base
        };
    }
    if !token_ok {
        return base;
    }
    match ConnectorLink::open(
        &connector.manifest,
        &connector.dir,
        &token_value,
        SPAWN_TIMEOUT,
    ) {
        Ok(link) => {
            let tool_count = link.tools().len();
            DoctorReport {
                tools_ok: true,
                tool_count,
                ..base
            }
        }
        Err(e) => DoctorReport {
            detail: e.to_string(),
            ..base
        },
    }
}

fn doctor_cmd(json: bool) -> i32 {
    let roots = roots();
    let (found, errors) = discover(&roots.iter().map(|p| p.as_path()).collect::<Vec<_>>());
    let store = connector_store();
    let reports: Vec<DoctorReport> = found
        .iter()
        .map(|c| doctor_one(store.as_ref(), c))
        .collect();
    let failed = reports
        .iter()
        .filter(|r| !r.transport_ok || !r.tools_ok)
        .count()
        + errors.len();
    if json {
        let out = json!({
            "connectors": reports.iter().map(|r| json!({
                "name": r.name,
                "kind": r.kind,
                "transport_ok": r.transport_ok,
                "token_required": r.token_required,
                "token": r.token_ok,
                "token_refreshed": r.refreshed,
                "tools_responding": r.tools_ok,
                "tool_count": r.tool_count,
                "detail": r.detail,
            })).collect::<Vec<_>>(),
            "discovery_errors": errors.len(),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return if failed == 0 { 0 } else { 1 };
    }
    for r in &reports {
        let status = if r.transport_ok && r.tools_ok {
            "ok"
        } else {
            "FAIL"
        };
        let token_note = if r.token_required && !r.token_ok {
            ", no token"
        } else if r.refreshed {
            ", token refreshed"
        } else {
            ""
        };
        if r.detail.is_empty() {
            println!("{status:4} {} ({} tools{token_note})", r.name, r.tool_count);
        } else {
            println!("{status:4} {}: {}{token_note}", r.name, r.detail);
        }
    }
    for e in &errors {
        eprintln!("warning: {}: {}", e.dir.display(), e.error);
    }
    if failed == 0 {
        0
    } else {
        1
    }
}

/// `audit`: query `<session-dir>/connectors-audit.jsonl` — the
/// append-only record of connector tool calls the Host writes.
fn audit_cmd(opts: AuditOptions, json: bool) -> i32 {
    let dir = match session_dir(&opts.session) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let path = dir.join("connectors-audit.jsonl");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            eprintln!("could not read {}: {e}", path.display());
            return 1;
        }
    };
    let mut rows: Vec<Value> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                if let Some(tool) = &opts.tool {
                    let row_tool = v.get("tool").and_then(Value::as_str).unwrap_or("");
                    if row_tool != tool {
                        continue;
                    }
                }
                rows.push(v);
            }
            Err(e) => {
                eprintln!("warning: {}:{}: bad audit line: {e}", path.display(), n + 1);
            }
        }
    }
    let start = rows.len().saturating_sub(opts.limit);
    let rows = &rows[start..];
    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
        return 0;
    }
    if rows.is_empty() {
        println!("no connector calls recorded for session {}", opts.session);
        return 0;
    }
    for row in rows {
        let get = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or("?");
        let ok = row.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let outcome = if ok {
            "ok".to_string()
        } else {
            format!(
                "FAIL{}",
                row.get("error")
                    .and_then(Value::as_str)
                    .map(|e| format!(": {e}"))
                    .unwrap_or_default()
            )
        };
        let ms = row
            .get("ms")
            .and_then(Value::as_u64)
            .map(|m| format!(" {m}ms"))
            .unwrap_or_default();
        println!(
            "{} {}.{} policy={} {}{}",
            get("ts"),
            get("connector"),
            get("tool"),
            get("policy"),
            outcome,
            ms
        );
    }
    0
}

/// Atomically replace the installed connector `dest` with the staged
/// directory: preserve the user's local `config.json` over the bundle's,
/// then rename dest aside, rename staging into place, and drop the
/// backup. If anything fails after the backup rename, the backup is
/// restored so the old install survives.
fn swap_install(dest: &Path, staging: &Path) -> Result<(), String> {
    let local_config = dest.join("config.json");
    if local_config.is_file() {
        std::fs::copy(&local_config, staging.join("config.json"))
            .map_err(|e| format!("could not preserve local config.json: {e}"))?;
    }
    if !dest.exists() {
        return std::fs::rename(staging, dest)
            .map_err(|e| format!("could not move staged install into place: {e}"));
    }
    let backup = dest.with_extension(format!("backup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::rename(dest, &backup).map_err(|e| format!("could not move old install aside: {e}"))?;
    if let Err(e) = std::fs::rename(staging, dest) {
        // Restore the old install; report the original error.
        let _ = std::fs::rename(&backup, dest);
        return Err(format!("could not move staged install into place: {e}"));
    }
    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::remove_dir_all(staging);
    Ok(())
}

/// `sync`: discover → resolve → install → verify → report. Without
/// `--registry` it verifies what's installed; with one it also
/// installs/updates every registry connector to its latest version.
fn sync_cmd(registry: Option<&str>, json: bool) -> i32 {
    let scan = sync_roots();
    let (_found, errors) = discover(&scan.iter().map(|p| p.as_path()).collect::<Vec<_>>());
    let store = connector_store();
    let mut installed: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut verified: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    if let Some(registry) = registry {
        let registry_dir = PathBuf::from(registry);
        // A typo'd registry path must fail loudly; a registry dir that
        // merely has no index.json yet is an empty (fresh) registry.
        if !registry_dir.is_dir() {
            eprintln!("registry not found: {registry}");
            return 1;
        }
        let index = match read_index(&registry_dir) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("could not read registry {registry}: {e}");
                return 1;
            }
        };
        // What counts as "installed" is the install dir only — a source
        // scaffold sitting in a registrar root is not an install.
        let (installed_found, _) = discover(&[install_root().as_path()]);
        for name in index.connectors.keys() {
            let (latest, entry) = match resolve(&registry_dir, name, None) {
                Ok(r) => r,
                Err(e) => {
                    failed.push((name.clone(), e.to_string()));
                    continue;
                }
            };
            let installed_version = installed_found
                .iter()
                .find(|c| &c.manifest.name == name)
                .map(|c| c.manifest.version.clone());
            let needs_install = match &installed_version {
                None => true,
                Some(v) => match semver::Version::parse(&latest) {
                    Ok(lv) => v < &lv,
                    Err(_) => latest != v.to_string(),
                },
            };
            if !needs_install {
                skipped.push((name.clone(), format!("already at latest ({latest})")));
                continue;
            }
            // Install the resolved bundle through the same verified path
            // as `install --registry`: digest-checked fetch, signature
            // verified against the registry's pinned publisher key.
            let (bundle, sig_path) = match fetch(&registry_dir, &entry) {
                Ok(f) => f,
                Err(e) => {
                    failed.push((name.clone(), e.to_string()));
                    continue;
                }
            };
            let sig = match read_signature(&sig_path) {
                Ok(s) => s,
                Err(e) => {
                    failed.push((name.clone(), format!("missing signature: {e}")));
                    continue;
                }
            };
            let dest = install_root().join(name);
            // Stage the verified bundle beside the install dir, then swap
            // it in atomically: a failed update must leave the old install
            // untouched, never a half-written directory.
            let staging =
                install_root().join(format!(".sync-staging-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&staging);
            match unpack_verified(&bundle, &sig, &entry.pubkey, &staging) {
                Ok(()) => match swap_install(&dest, &staging) {
                    Ok(()) => match installed_version {
                        None => installed.push(format!("{name} {latest}")),
                        Some(old) => updated.push(format!("{name} {old} → {latest}")),
                    },
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&staging);
                        failed.push((name.clone(), format!("install failed: {e}")));
                    }
                },
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    failed.push((name.clone(), format!("signature verification failed: {e}")));
                }
            }
        }
    }

    // Verify: manifest valid (discover parsed it), transport present,
    // token present when the manifest requires auth. Lighter than doctor
    // — no process is spawned.
    let scan2 = sync_roots();
    let (found, _) = discover(&scan2.iter().map(|p| p.as_path()).collect::<Vec<_>>());
    for c in &found {
        let name = &c.manifest.name;
        let transport_ok = match &c.manifest.kind {
            ConnectorKind::McpStdio => executable_for(c).is_file(),
            ConnectorKind::McpHttp => {
                mcp_url_from_config(&c.dir).is_some_and(|u| !u.trim().is_empty())
            }
            ConnectorKind::Builtin => true,
        };
        let token_ok = c.manifest.auth_flow == AuthFlow::None
            || load_connector_token(store.as_ref(), name)
                .unwrap_or(None)
                .is_some_and(|t| !t.trim().is_empty());
        if transport_ok && token_ok {
            verified.push(name.clone());
        } else {
            let mut why = Vec::new();
            if !transport_ok {
                why.push("transport missing".to_string());
            }
            if !token_ok {
                why.push(format!("not connected (`supercli connector connect {name}`)"));
            }
            failed.push((name.clone(), why.join(", ")));
        }
    }
    for e in &errors {
        failed.push((e.dir.display().to_string(), e.error.to_string()));
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "installed": installed,
                "updated": updated,
                "skipped": skipped.iter().map(|(n, r)| json!({"name": n, "reason": r})).collect::<Vec<_>>(),
                "verified": verified,
                "failed": failed.iter().map(|(n, r)| json!({"name": n, "reason": r})).collect::<Vec<_>>(),
            }))
            .unwrap()
        );
    } else {
        for name in &installed {
            println!("installed {name}");
        }
        for name in &updated {
            println!("updated {name}");
        }
        for (name, reason) in &skipped {
            println!("skipped {name}: {reason}");
        }
        for name in &verified {
            println!("verified {name}");
        }
        for (name, reason) in &failed {
            println!("FAILED {name}: {reason}");
        }
        if installed.is_empty() && updated.is_empty() && failed.is_empty() {
            println!(
                "sync: {} connector(s) verified, nothing to install",
                verified.len()
            );
        }
    }
    if failed.is_empty() {
        0
    } else {
        1
    }
}

/// Copy a directory tree. Symlinks are followed (copied as regular files);
/// unix permission bits are preserved.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        for entry in std::fs::read_dir(&s)? {
            let entry = entry?;
            let src_p = entry.path();
            let dst_p = d.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                std::fs::create_dir_all(&dst_p)?;
                stack.push((src_p, dst_p));
            } else {
                std::fs::copy(&src_p, &dst_p)?;
                #[cfg(unix)]
                if let Ok(meta) = std::fs::metadata(&src_p) {
                    let _ = std::fs::set_permissions(&dst_p, meta.permissions());
                }
            }
        }
    }
    Ok(())
}

fn install_cmd(opts: &InstallOptions, json: bool) -> i32 {
    // Registry installs resolve a signed bundle; a path ending in
    // .supercli-connector is a bundle file; everything else is a directory
    // or a discovered connector name.
    if let Some(registry) = &opts.registry {
        return install_from_registry(&opts.source, registry, opts, json);
    }
    let source_path = PathBuf::from(&opts.source);
    if source_path
        .extension()
        .is_some_and(|e| e == "supercli-connector")
    {
        return install_from_bundle(&source_path, opts, json);
    }
    install_from_dir(&opts.source, opts, json)
}

/// Collect the non-secret config for an install: `--form` renders the
/// manifest's `config_schema` as an interactive questionnaire, otherwise
/// the key=value pairs are validated against the schema (when the
/// manifest has one). Returns `None` when there is nothing to write.
fn collect_install_config(
    manifest: &ConnectorManifest,
    opts: &InstallOptions,
) -> Result<Option<Map<String, Value>>, String> {
    if opts.form {
        if manifest.config_schema.is_empty() {
            eprintln!(
                "warning: {} has no config_schema — nothing to ask",
                manifest.name
            );
            return Ok(None);
        }
        return run_form(&manifest.config_schema)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    if opts.config_pairs.is_empty() {
        return Ok(None);
    }
    validated_config(manifest, &opts.config_pairs).map(Some)
}

/// Split `key=value` words into raw pairs (the schema coerces the
/// values, so no JSON pre-parsing happens here).
fn split_pairs(pairs: &[String]) -> Result<Vec<(String, String)>, String> {
    pairs
        .iter()
        .map(|word| {
            word.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| format!("expected key=value, got {word:?}"))
        })
        .collect()
}

/// Validate raw key=value pairs against the schema and coerce them to
/// JSON values. When the manifest has no schema, fall back to the
/// JSON-coercing parse (accepts any key).
fn validated_config(
    manifest: &ConnectorManifest,
    pairs: &[String],
) -> Result<Map<String, Value>, String> {
    if manifest.config_schema.is_empty() {
        let words: Vec<&str> = pairs.iter().map(String::as_str).collect();
        return parse_kv(&words);
    }
    let raw = split_pairs(pairs)?;
    validate_pairs(&manifest.config_schema, &raw).map_err(|e| e.to_string())
}

/// Write config.json atomically into an installed connector dir.
fn write_config_json(dir: &Path, config: &Map<String, Value>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&Value::Object(config.clone()))
        .map_err(|e| format!("could not serialize config.json: {e}"))?;
    let tmp = dir.join("config.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("could not write config.json: {e}"))?;
    std::fs::rename(&tmp, dir.join("config.json"))
        .map_err(|e| format!("could not write config.json: {e}"))?;
    Ok(())
}

fn install_report(manifest: &ConnectorManifest, dest: &Path, json: bool, extra: &str) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": manifest.name,
                "version": manifest.version.to_string(),
                "dir": dest,
                "note": extra,
            }))
            .unwrap()
        );
    } else {
        println!(
            "installed {} {} → {}",
            manifest.name,
            manifest.version,
            dest.display()
        );
        if !extra.is_empty() {
            println!("{extra}");
        }
        if dest.join("config.json").is_file() {
            println!("wrote config.json (non-secrets only)");
        }
    }
}

/// Install from a directory holding connector.toml, or the name of a
/// discovered connector (copied into the install dir).
fn install_from_dir(source: &str, opts: &InstallOptions, json: bool) -> i32 {
    if opts.require_signature {
        eprintln!(
            "refusing: --require-signature cannot verify a loose directory — \
             install from a registry or a signed .supercli-connector bundle instead"
        );
        return 1;
    }
    let source_dir: PathBuf = {
        let path = PathBuf::from(source);
        if path.join("connector.toml").is_file() {
            path
        } else if path.is_dir() {
            eprintln!("{source:?} is a directory but has no connector.toml");
            return 1;
        } else {
            match find_connector(source) {
                Ok(c) => c.dir.clone(),
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            }
        }
    };
    let manifest = match read_dir_manifest(&source_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    if manifest.kind == ConnectorKind::McpStdio && !source_dir.join("connector").is_file() {
        eprintln!("source is missing the `connector` executable (mcp-stdio requires one)");
        return 1;
    }
    // Fail fast on bad config before touching the filesystem.
    let config = match collect_install_config(&manifest, opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let dest = install_root().join(&manifest.name);
    if dest.exists() {
        eprintln!(
            "{:?} is already installed at {} — remove the directory first to reinstall",
            manifest.name,
            dest.display()
        );
        return 1;
    }
    if let Err(e) = copy_dir(&source_dir, &dest) {
        eprintln!("install failed: {e}");
        return 1;
    }
    // Non-secret config lands in config.json; secrets belong in the
    // keychain via `connect`, never on disk.
    if let Some(config) = config {
        if let Err(e) = write_config_json(&dest, &config) {
            eprintln!("installed, but {e}");
            return 1;
        }
    }
    install_report(
        &manifest,
        &dest,
        json,
        "unsigned source: install only from directories you trust",
    );
    0
}

fn read_dir_manifest(dir: &Path) -> Result<ConnectorManifest, String> {
    let manifest_text = std::fs::read_to_string(dir.join("connector.toml"))
        .map_err(|e| format!("could not read connector.toml: {e}"))?;
    parse_manifest(&manifest_text).map_err(|e| format!("invalid connector.toml: {e}"))
}

/// The trusted publisher key for a bundle install: `--pubkey <file>`
/// (the `<id>.pub` format or raw base64) wins over `--key-id <id>`
/// (default "default"), which loads the key from the local keys dir.
fn bundle_trust_key(opts: &InstallOptions) -> Result<String, String> {
    if let Some(file) = &opts.pubkey {
        let text = std::fs::read_to_string(file)
            .map_err(|e| format!("could not read --pubkey {}: {e}", Path::new(file).display()))?;
        let text = text.trim();
        let prefixed = if text.starts_with("supercli-ed25519-pub-v1:") {
            text.to_string()
        } else {
            format!("supercli-ed25519-pub-v1:{text}")
        };
        let key = parse_public_key(&prefixed).map_err(|e| format!("bad --pubkey: {e}"))?;
        return Ok(public_key_base64(&key));
    }
    let key = load_public_key(opts.key_id())
        .map_err(|e| format!("no trusted key: {e} (pass --pubkey <file> or generate one with `supercli connector keygen`)"))?;
    Ok(public_key_base64(&key))
}

/// Install from a signed `.supercli-connector` bundle file, verifying the
/// `.sig` sidecar against the trusted publisher key before unpacking.
fn install_from_bundle(bundle_path: &Path, opts: &InstallOptions, json: bool) -> i32 {
    if !bundle_path.is_file() {
        eprintln!("no bundle file at {}", bundle_path.display());
        return 1;
    }
    let sig_path = bundle_path.with_extension("supercli-connector.sig");
    let bundle = match std::fs::read(bundle_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("could not read bundle: {e}");
            return 1;
        }
    };
    let sig = match read_signature(&sig_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no valid signature at {}: {e}", sig_path.display());
            return 1;
        }
    };
    let trusted = match bundle_trust_key(opts) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let manifest = match bundle_manifest(&bundle) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("bad bundle: {e}");
            return 1;
        }
    };
    let config = match collect_install_config(&manifest, opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let dest = install_root().join(&manifest.name);
    if dest.exists() {
        eprintln!(
            "{:?} is already installed at {} — remove the directory first to reinstall",
            manifest.name,
            dest.display()
        );
        return 1;
    }
    // Unpack to a staging dir, then rename into place so a failed
    // install never leaves a half-written connector behind.
    let staging = install_root().join(format!(".staging-{}-{}", manifest.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(e) = unpack_verified(&bundle, &sig, &trusted, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        eprintln!("signature verification failed: {e} — install refused");
        return 1;
    }
    if let Err(e) = std::fs::rename(&staging, &dest) {
        let _ = std::fs::remove_dir_all(&staging);
        eprintln!("install failed: {e}");
        return 1;
    }
    if let Some(config) = config {
        if let Err(e) = write_config_json(&dest, &config) {
            eprintln!("installed, but {e}");
            return 1;
        }
    }
    install_report(&manifest, &dest, json, "bundle signature verified");
    0
}

/// Roots the sync pass scans: the registrar scan roots (which always
/// include the install dir).
fn sync_roots() -> Vec<PathBuf> {
    roots()
}

/// Install from a registry: `name` or `name@version`. The bundle digest
/// is checked on fetch and the signature is verified against the
/// registry's pinned publisher key before unpacking — always, with no
/// opt-out.
fn install_from_registry(source: &str, registry: &str, opts: &InstallOptions, json: bool) -> i32 {
    let registry_dir = PathBuf::from(registry);
    let (name, version) = match source.split_once('@') {
        Some((n, v)) => (n, Some(v)),
        None => (source, None),
    };
    let (resolved_version, entry) = match resolve(&registry_dir, name, version) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let (bundle, sig_path) = match fetch(&registry_dir, &entry) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let sig = match read_signature(&sig_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("registry is missing the bundle signature: {e}");
            return 1;
        }
    };
    let manifest = match bundle_manifest(&bundle) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("bad bundle: {e}");
            return 1;
        }
    };
    let config = match collect_install_config(&manifest, opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let dest = install_root().join(&manifest.name);
    if dest.exists() {
        eprintln!(
            "{:?} is already installed at {} — remove the directory first to reinstall",
            manifest.name,
            dest.display()
        );
        return 1;
    }
    let staging = install_root().join(format!(".staging-{}-{}", manifest.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(e) = unpack_verified(&bundle, &sig, &entry.pubkey, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        eprintln!("signature verification failed: {e} — install refused");
        return 1;
    }
    if let Err(e) = std::fs::rename(&staging, &dest) {
        let _ = std::fs::remove_dir_all(&staging);
        eprintln!("install failed: {e}");
        return 1;
    }
    if let Some(config) = config {
        if let Err(e) = write_config_json(&dest, &config) {
            eprintln!("installed, but {e}");
            return 1;
        }
    }
    install_report(
        &manifest,
        &dest,
        json,
        &format!("registry {registry}, version {resolved_version}, signature verified (publisher key pinned)"),
    );
    0
}

/// `keygen`: generate an Ed25519 publisher keypair.
fn keygen_cmd(key_id: &str, json: bool) -> i32 {
    match keygen(key_id) {
        Ok((secret_path, pub_path)) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "key_id": key_id,
                        "secret": secret_path,
                        "public": pub_path,
                    }))
                    .unwrap()
                );
            } else {
                println!("publisher keypair {key_id:?}:");
                println!("  secret: {}", secret_path.display());
                println!("  public: {}", pub_path.display());
                println!(
                    "keep the .key file private; publish with the .pub key pinned by the registry."
                );
            }
            0
        }
        Err(e) => {
            eprintln!("keygen failed: {e}");
            1
        }
    }
}

/// `pack`: build a signed bundle from an installed/discovered connector.
fn pack_cmd(name: &str, out: Option<&str>, key_id: &str, json: bool) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let out_dir = match out {
        Some(o) => PathBuf::from(o),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    match pack(&connector.dir, &out_dir, key_id) {
        Ok((bundle_path, sig_path)) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "name": connector.manifest.name,
                        "version": connector.manifest.version.to_string(),
                        "bundle": bundle_path,
                        "signature": sig_path,
                        "key_id": key_id,
                    }))
                    .unwrap()
                );
            } else {
                println!("packed {} → {}", bundle_path.display(), sig_path.display());
            }
            0
        }
        Err(e) => {
            eprintln!("pack failed: {e}");
            1
        }
    }
}

/// `publish`: pack to a temp dir and publish into the registry.
fn publish_cmd(name: &str, registry: &str, key_id: &str, json: bool) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let tmp = std::env::temp_dir().join(format!("supercli-pack-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (bundle_path, sig_path) = match pack(&connector.dir, &tmp, key_id) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("pack failed: {e}");
            return 1;
        }
    };
    let result = publish(Path::new(registry), &bundle_path, &sig_path);
    let _ = std::fs::remove_dir_all(&tmp);
    match result {
        Ok((published_name, version)) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "name": published_name,
                        "version": version,
                        "registry": registry,
                        "key_id": key_id,
                    }))
                    .unwrap()
                );
            } else {
                println!("published {published_name} {version} → {registry}");
            }
            0
        }
        Err(e) => {
            eprintln!("publish failed: {e}");
            1
        }
    }
}

/// `config`: show or update an installed connector's config.json.
/// Without --form and without pairs it just prints the current config.
fn config_cmd(name: &str, form: bool, pairs: &[String], json: bool) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let path = connector.dir.join("config.json");
    let mut config: Map<String, Value> = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => map,
            _ => {
                eprintln!(
                    "{} is not a JSON object — refusing to merge",
                    path.display()
                );
                return 1;
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(e) => {
            eprintln!("could not read {}: {e}", path.display());
            return 1;
        }
    };
    if !form && pairs.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&Value::Object(config)).unwrap()
            );
        } else if config.is_empty() {
            println!("{name}: no config.json");
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&Value::Object(config)).unwrap()
            );
        }
        return 0;
    }
    if form {
        if connector.manifest.config_schema.is_empty() {
            eprintln!("{name} has no config_schema — nothing to ask");
            return 1;
        }
        match run_form(&connector.manifest.config_schema) {
            Ok(answered) => config = merge_config(&config, answered),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    if !pairs.is_empty() {
        match validated_config(&connector.manifest, pairs) {
            Ok(parsed) => config = merge_config(&config, parsed),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    }
    if let Err(e) = write_config_json(&connector.dir, &config) {
        eprintln!("{e}");
        return 1;
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": name,
                "config": Value::Object(config),
            }))
            .unwrap()
        );
    } else {
        println!("{name}: config.json updated (secrets still belong in the keychain, not here).");
    }
    0
}

fn prompt_token(name: &str) -> Result<String, String> {
    use std::io::Write;
    eprint!("API token for {name}: ");
    std::io::stderr()
        .flush()
        .map_err(|e| format!("could not prompt: {e}"))?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("could not read token: {e}"))?;
    if line.is_empty() {
        return Err("no token entered — pass --token <value> for non-interactive use".to_string());
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

fn connect_cmd(name: &str, token_flag: Option<String>, json: bool) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let store = connector_store();
    match connector.manifest.auth_flow {
        AuthFlow::None => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({ "name": name, "auth": "none", "connected": true })
                    )
                    .unwrap()
                );
            } else {
                println!("{name}: auth flow is \"none\" — no token needed, nothing stored.");
            }
            0
        }
        AuthFlow::ApiKey => {
            let token = match token_flag {
                Some(t) => t,
                None => match prompt_token(name) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("{e}");
                        return 1;
                    }
                },
            };
            if token.trim().is_empty() {
                eprintln!("refusing to store an empty token");
                return 1;
            }
            match store_connector_token(store.as_ref(), name, &token) {
                Ok(()) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(
                                &json!({ "name": name, "auth": "api-key", "connected": true })
                            )
                            .unwrap()
                        );
                    } else {
                        println!("{name}: token stored in the keychain.");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("could not store token: {e}");
                    1
                }
            }
        }
        AuthFlow::OAuth2 => {
            // The browser dance: open the provider's authorize URL (PKCE),
            // wait on a loopback callback, exchange the code for tokens.
            let endpoints = match &connector.manifest.oauth {
                Some(e) => e.clone(),
                None => {
                    eprintln!(
                        "{name}: auth.flow is \"oauth2\" but the manifest has no [oauth] \
                         section (authorize_url, token_url)"
                    );
                    return 1;
                }
            };
            let client_id = config_string(&connector.dir, "client_id").unwrap_or_default();
            if client_id.trim().is_empty() {
                eprintln!(
                    "{name}: OAuth2 needs a client_id — put it in the installed config.json \
                     (`supercli connector config {name} --form` asks for it when the schema declares it)"
                );
                return 1;
            }
            if token_flag.is_some() {
                eprintln!(
                    "warning: --token is ignored for OAuth2 connectors (the dance mints the token)"
                );
            }
            println!("{name}: opening the browser to authorize…");
            match run_oauth_dance(
                &endpoints,
                client_id.trim(),
                &connector.manifest.auth_scopes,
                DANCE_TIMEOUT,
            ) {
                Ok(set) => match store_connector_token(store.as_ref(), name, &set.to_stored()) {
                    Ok(()) => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(
                                    &json!({ "name": name, "auth": "oauth2", "connected": true })
                                )
                                .unwrap()
                            );
                        } else {
                            println!("{name}: OAuth2 tokens stored in the keychain (auto-refreshed before expiry).");
                        }
                        0
                    }
                    Err(e) => {
                        eprintln!("authorized, but could not store the token: {e}");
                        1
                    }
                },
                Err(e) => {
                    eprintln!("OAuth2 failed: {e}");
                    1
                }
            }
        }
    }
}

fn disconnect_cmd(name: &str, json: bool) -> i32 {
    if let Err(e) = find_connector(name) {
        eprintln!("{e}");
        return 1;
    }
    let store = connector_store();
    let had_token = load_connector_token(store.as_ref(), name)
        .unwrap_or(None)
        .is_some();
    match delete_connector_token(store.as_ref(), name) {
        Ok(()) => {
            // Revocation is one verb: the token is gone and the tools come
            // off every session, so a disconnected connector can never be
            // reached through a stale attachment.
            let detached = detach_from_all_sessions(name);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({ "name": name, "disconnected": true, "had_token": had_token, "detached_sessions": detached })
                    )
                    .unwrap()
                );
            } else if had_token {
                println!("{name}: token revoked.");
            } else {
                println!("{name}: was not connected — nothing to revoke.");
            }
            if !json && detached > 0 {
                println!(
                    "{name}: detached from {detached} session{}.",
                    if detached == 1 { "" } else { "s" }
                );
            }
            0
        }
        Err(e) => {
            eprintln!("could not revoke token: {e}");
            1
        }
    }
}

/// Resolve and validate a session dir from a `--session <id>` value. The
/// id is restricted to `[A-Za-z0-9_-]` so it can never escape the
/// sessions root via path traversal.
fn session_dir(session_id: &str) -> Result<PathBuf, String> {
    if session_id.is_empty()
        || !session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "bad session id {session_id:?}: expected [A-Za-z0-9_-]+"
        ));
    }
    let dir = supercli_core::app_paths::app_sessions_root().join(session_id);
    if !dir.join("manifest.json").is_file() {
        return Err(format!(
            "no session {session_id:?} (expected {})",
            dir.join("manifest.json").display()
        ));
    }
    Ok(dir)
}

/// The connector must be connected before its tools can attach to a
/// session — same token semantics as `doctor`: `none` never needs one,
/// `api-key` and `oauth2` need a stored token (the dance may have minted
/// a refreshable set).
fn require_connected(
    store: &dyn CredentialStore,
    connector: &DiscoveredConnector,
) -> Result<(), String> {
    let name = &connector.manifest.name;
    match connector.manifest.auth_flow {
        AuthFlow::None => Ok(()),
        AuthFlow::ApiKey | AuthFlow::OAuth2 => match load_connector_token(store, name) {
            Ok(Some(t)) if !t.trim().is_empty() => Ok(()),
            Ok(_) => Err(format!(
                "{name} is not connected — run `supercli connector connect {name}` first"
            )),
            Err(e) => Err(format!("could not read token for {name}: {e}")),
        },
    }
}

fn enable_cmd(
    name: &str,
    session_id: &str,
    policies: &HashMap<String, ApprovalPolicy>,
    json: bool,
) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let dir = match session_dir(session_id) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let store = connector_store();
    if let Err(e) = require_connected(store.as_ref(), &connector) {
        eprintln!("{e}");
        return 1;
    }
    // Policy overrides may only tighten: the manifest default is the
    // ceiling, and the tool must be one the connector provides.
    for (tool, policy) in policies {
        if !connector.manifest.provides.iter().any(|t| t == tool) {
            eprintln!("{name} provides no tool {tool:?}");
            return 1;
        }
        let ceiling = connector.manifest.default_policy(tool);
        if *policy < ceiling {
            eprintln!(
                "cannot loosen {tool:?} to {policy:?}: the manifest default is {ceiling:?} (tighten only)"
            );
            return 1;
        }
    }
    match enable_attachment(&dir, name, policies.clone()) {
        Ok(is_new) => {
            if json {
                let policy_json: Map<String, Value> = policies
                    .iter()
                    .map(|(t, p)| (t.clone(), Value::String(format!("{p:?}").to_lowercase())))
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "name": name,
                        "session": session_id,
                        "enabled": true,
                        "new": is_new,
                        "policy": policy_json,
                    }))
                    .unwrap()
                );
            } else if is_new {
                println!("{name}: attached to session {session_id}.");
            } else {
                println!("{name}: already attached to session {session_id} — policy updated.");
            }
            0
        }
        Err(e) => {
            eprintln!("could not attach {name} to session {session_id}: {e}");
            1
        }
    }
}

fn disable_cmd(name: &str, session_id: &str, json: bool) -> i32 {
    let dir = match session_dir(session_id) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    match disable_attachment(&dir, name) {
        Ok(was_enabled) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "name": name,
                        "session": session_id,
                        "disabled": true,
                        "was_enabled": was_enabled,
                    }))
                    .unwrap()
                );
            } else if was_enabled {
                println!("{name}: detached from session {session_id}.");
            } else {
                println!("{name}: was not attached to session {session_id} — nothing to do.");
            }
            0
        }
        Err(e) => {
            eprintln!("could not detach {name} from session {session_id}: {e}");
            1
        }
    }
}

/// Best-effort detach from every session dir (for `disconnect`'s
/// revocation contract). Returns the number of sessions detached.
fn detach_from_all_sessions(name: &str) -> usize {
    let root = supercli_core::app_paths::app_sessions_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return 0;
    };
    let mut detached = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        // Best effort: a corrupt record in one session must not block the
        // others, and the token is already revoked either way.
        if disable_attachment(&dir, name).unwrap_or(false) {
            detached += 1;
        }
    }
    detached
}

/// Parse `key=value` words into a JSON object; values that parse as JSON
/// (true, 42, ["a"], {"k":1}) become JSON, anything else a string.
fn parse_kv(rest: &[&str]) -> Result<Map<String, Value>, String> {
    let mut arguments = Map::new();
    for word in rest {
        let (key, value) = word
            .split_once('=')
            .ok_or_else(|| format!("expected key=value, got {word:?}"))?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("bad argument key {key:?}"));
        }
        let value: Value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        arguments.insert(key.to_string(), value);
    }
    Ok(arguments)
}

fn run_cmd(name: &str, tool: &str, rest: &[&str], json: bool) -> i32 {
    let connector = match find_connector(name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    if !connector.manifest.provides.iter().any(|t| t == tool) {
        eprintln!(
            "{name:?} does not provide tool {tool:?} (manifest `tools.provides` is a closed list)"
        );
        return 1;
    }
    // Non-interactive: only Allow tools run. Anything stricter needs a
    // session (approval prompt) or a looser manifest default.
    let policy = tool_policy(&connector, tool);
    if policy != ApprovalPolicy::Allow {
        eprintln!(
            "refusing: tool {tool:?} has effective policy {policy:?} (needs approval); \
             `supercli connector run` only invokes Allow tools"
        );
        return 1;
    }
    let arguments = match parse_kv(rest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let store = connector_store();
    let token = match resolve_connector_token(
        &connector.manifest,
        &connector.dir,
        store.as_ref(),
        name,
        SPAWN_TIMEOUT,
    ) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{name}: {e}");
            return 1;
        }
    };
    let mut link =
        match ConnectorLink::open(&connector.manifest, &connector.dir, &token, SPAWN_TIMEOUT) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("could not open connector {name:?}: {e}");
                return 1;
            }
        };
    let args: HashMap<String, Value> = arguments.into_iter().collect();
    match link.call(tool, args) {
        Ok(result) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else if let Value::String(s) = &result {
                println!("{s}");
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
            0
        }
        Err(e) => {
            eprintln!("tool call failed: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use supercli_connector::CONNECTORS_KEYCHAIN_ENV;

    const MANIFEST: &str = r#"
[connector]
name = "stubby"
version = "0.1.0"
display_name = "Stubby"
description = "test connector"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["stubby.echo"]
"#;

    const MANIFEST_ALLOW: &str = r#"
[connector]
name = "allowy"
version = "0.1.0"
display_name = "Allowy"
description = "test connector with an Allow tool"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["allowy.echo"]

[policy]
"allowy.echo" = "allow"
"#;

    /// A stub MCP server (Python) implementing initialize, tools/list,
    /// tools/call. Mirrors the stub in supercli-connector's process tests.
    /// The served tool name comes from argv[1].
    const STUB: &str = r#"import json, sys
TOOL = sys.argv[1]
TOOLS = [{"name": TOOL, "description": "echo", "inputSchema": {"type": "object"}}]
def respond(mid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        respond(mid, {"content": [{"type": "text", "text": "echo:" + json.dumps(msg["params"]["arguments"])}]})
"#;

    /// Same as STUB but echoes the token it received via the environment,
    /// so tests can prove the keychain token reaches the process.
    const STUB_TOKEN_ECHO: &str = r#"import json, os, sys
TOOL = sys.argv[1]
TOKEN = os.environ.get("SUPERCLI_CONNECTOR_TOKEN", "")
TOOLS = [{"name": TOOL, "description": "echo", "inputSchema": {"type": "object"}}]
def respond(mid, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": result}) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        respond(mid, {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "stub", "version": "0"}})
    elif method == "tools/list":
        respond(mid, {"tools": TOOLS})
    elif method == "tools/call":
        respond(mid, {"content": [{"type": "text", "text": "echo:" + json.dumps(msg["params"]["arguments"]) + "|token:" + TOKEN}]})
"#;

    fn manifest_for(name: &str, flow: &str, tool: &str, policy: Option<&str>) -> String {
        let policy = policy
            .map(|p| format!("\n[policy]\n\"{tool}\" = \"{p}\"\n"))
            .unwrap_or_default();
        format!(
            r#"
[connector]
name = "{name}"
version = "0.1.0"
display_name = "{name}"
description = "test connector"
kind = "mcp-stdio"

[auth]
flow = "{flow}"

[tools]
provides = ["{tool}"]
{policy}"#
        )
    }

    struct Fixture {
        dir: PathBuf,
        install_dir: PathBuf,
        home_dir: PathBuf,
        keys_dir: PathBuf,
        // Serializes every fixture test: the env vars below are
        // process-global, so parallel fixtures race each other no matter
        // how carefully each pins them. Held until the test's fixture
        // drops.
        _env_guard: std::sync::MutexGuard<'static, ()>,
    }

    static FIXTURE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    /// Process-global env vars make parallel fixtures race; one fixture at
    /// a time.
    static FIXTURE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl Fixture {
        fn next_id() -> u64 {
            FIXTURE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        }

        fn write_connector(
            dir: &std::path::Path,
            name: &str,
            manifest: &str,
            tool: &str,
            stub: &str,
        ) {
            let conn = dir.join(name);
            std::fs::create_dir_all(&conn).unwrap();
            std::fs::write(conn.join("connector.toml"), manifest).unwrap();
            // The stub must live in a file: feeding it via a heredoc would
            // consume the stdin the MCP server itself reads JSON-RPC from.
            let script = conn.join("stub.py");
            std::fs::write(&script, stub).unwrap();
            let exe = conn.join("connector");
            let wrapper = format!("#!/bin/sh\nexec python3 {} {tool}\n", script.display());
            std::fs::write(&exe, wrapper).unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn new() -> Self {
            // Unique per test: tests in one binary share the process id.
            // Take the env lock first: every fixture below mutates
            // process-global env vars. Recover from a poisoned lock so one
            // failing test can't cascade into unrelated failures.
            let env_guard = FIXTURE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let n = Self::next_id();
            let dir = std::env::temp_dir()
                .join(format!("supercli-conn-cli-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let install_dir = dir.join("installed");
            let home_dir = dir.join("home");
            let keys_dir = dir.join("keys");
            Self::write_connector(&dir, "stubby", MANIFEST, "stubby.echo", STUB);
            Self::write_connector(&dir, "allowy", MANIFEST_ALLOW, "allowy.echo", STUB);
            // Point the CLI at the fixture roots for the duration of the test.
            // The memory keychain override keeps tokens out of the real
            // keychain; the install dir keeps installs out of ~/.supercli;
            // SUPERCLI_HOME keeps session dirs out of the real ~/.supercli; the
            // keys dir keeps publisher keys out of the real ~/.supercli.
            std::env::set_var("SUPERCLI_CONNECTORS_DIR", dir.as_os_str());
            std::env::set_var("SUPERCLI_CONNECTORS_INSTALL_DIR", install_dir.as_os_str());
            std::env::set_var(CONNECTORS_KEYCHAIN_ENV, "memory");
            std::env::set_var("SUPERCLI_HOME", home_dir.as_os_str());
            std::env::set_var("SUPERCLI_CONNECTOR_KEYS_DIR", keys_dir.as_os_str());
            Self {
                dir,
                install_dir,
                home_dir,
                keys_dir,
                _env_guard: env_guard,
            }
        }

        /// Re-assert this fixture's env vars. Tests share one process, so a
        /// parallel test's `Fixture::new` can clobber them between our
        /// calls — pin right before any env-dependent operation.
        fn pin_env(&self) {
            std::env::set_var("SUPERCLI_CONNECTORS_DIR", self.dir.as_os_str());
            std::env::set_var(
                "SUPERCLI_CONNECTORS_INSTALL_DIR",
                self.install_dir.as_os_str(),
            );
            std::env::set_var(CONNECTORS_KEYCHAIN_ENV, "memory");
            std::env::set_var("SUPERCLI_HOME", self.home_dir.as_os_str());
            std::env::set_var("SUPERCLI_CONNECTOR_KEYS_DIR", self.keys_dir.as_os_str());
        }

        /// Scaffold a fake session dir: `app-sessions/<id>/manifest.json`.
        fn make_session(&self, id: &str) -> PathBuf {
            self.pin_env();
            let dir = self.home_dir.join("app-sessions").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("manifest.json"), r#"{"session":{}}"#).unwrap();
            dir
        }

        /// Read back the attachment record for a session dir.
        fn attachments(&self, session_dir: &Path) -> supercli_connector::SessionAttachments {
            self.pin_env();
            supercli_connector::read_attachments(session_dir).unwrap()
        }

        /// A uniquely named api-key connector (the shared memory keychain
        /// makes fixed names racy across parallel tests).
        fn api_key_connector(&self) -> (String, String) {
            let name = format!("keyedy-{}", Self::next_id());
            let tool = format!("{name}.echo");
            let manifest = manifest_for(&name, "api-key", &tool, Some("allow"));
            Self::write_connector(&self.dir, &name, &manifest, &tool, STUB_TOKEN_ECHO);
            self.pin_env();
            (name, tool)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
            std::env::remove_var("SUPERCLI_CONNECTORS_DIR");
            std::env::remove_var("SUPERCLI_CONNECTORS_INSTALL_DIR");
            std::env::remove_var(CONNECTORS_KEYCHAIN_ENV);
            std::env::remove_var("SUPERCLI_HOME");
            std::env::remove_var("SUPERCLI_CONNECTOR_KEYS_DIR");
        }
    }

    /// Test helper: install with plain options (no registry, no form).
    fn install_test(source: &str, pairs: &[&str], json: bool) -> i32 {
        install_cmd(
            &InstallOptions {
                source: source.to_string(),
                config_pairs: pairs.iter().map(|s| s.to_string()).collect(),
                registry: None,
                form: false,
                require_signature: false,
                pubkey: None,
                key_id: None,
            },
            json,
        )
    }

    #[test]
    fn discover_finds_fixtures() {
        let fx = Fixture::new();
        let roots = roots();
        assert_eq!(roots[0], fx.dir, "override root comes first");
        assert!(
            roots.contains(&fx.install_dir),
            "install dir always scanned"
        );
        let (found, errors) = discover(&[fx.dir.as_path()]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(found.len(), 2);
        let names: Vec<&str> = found.iter().map(|c| c.manifest.name.as_str()).collect();
        assert!(names.contains(&"stubby"));
        assert!(names.contains(&"allowy"));
    }

    #[test]
    fn parse_kv_json_coercion() {
        let args = parse_kv(&["a=1", "b=true", "c=hello", "d={\"x\": 2}"]).unwrap();
        assert_eq!(args["a"], json!(1));
        assert_eq!(args["b"], json!(true));
        assert_eq!(args["c"], json!("hello"));
        assert_eq!(args["d"], json!({"x": 2}));
        assert!(parse_kv(&["noequals"]).is_err());
        assert!(parse_kv(&["bad-key!=1"]).is_err());
    }

    #[test]
    fn doctor_report_ok_for_stubs() {
        let fx = Fixture::new();
        let store = connector_store();
        let (found, _) = discover(&[fx.dir.as_path()]);
        assert_eq!(found.len(), 2);
        for c in &found {
            let report = doctor_one(store.as_ref(), c);
            // auth.flow = "none": no token required, tools must respond.
            assert!(!report.token_required, "{}", report.name);
            assert!(report.token_ok, "{}: {}", report.name, report.detail);
            assert!(report.transport_ok, "{}: {}", report.name, report.detail);
            assert!(report.tools_ok, "{}: {}", report.name, report.detail);
            assert_eq!(report.tool_count, 1);
        }
    }

    #[test]
    fn doctor_fails_for_unconnected_api_key_connector() {
        let fx = Fixture::new();
        let (name, _) = fx.api_key_connector();
        let store = connector_store();
        fx.pin_env();
        let connector = find_connector(&name).expect("fixture connector");
        let report = doctor_one(store.as_ref(), &connector);
        assert!(report.token_required);
        assert!(!report.token_ok);
        assert!(!report.tools_ok, "tools must not be probed without a token");
        assert!(
            report.detail.contains("connect"),
            "detail should name the fix: {}",
            report.detail
        );
        // And the command surfaces the failure in its exit status.
        fx.pin_env();
        assert_eq!(doctor_cmd(true), 1);
    }

    #[test]
    fn connect_disconnect_lifecycle() {
        let fx = Fixture::new();
        let (name, tool) = fx.api_key_connector();
        let store = connector_store();

        // Not connected at first.
        assert_eq!(load_connector_token(store.as_ref(), &name).unwrap(), None);

        // connect --token stores it.
        fx.pin_env();
        assert_eq!(connect_cmd(&name, Some("sekret-1".to_string()), true), 0);
        assert_eq!(
            load_connector_token(store.as_ref(), &name).unwrap(),
            Some("sekret-1".to_string())
        );

        // doctor is now green and the token reaches the process env.
        fx.pin_env();
        let connector = find_connector(&name).expect("fixture connector");
        let report = doctor_one(store.as_ref(), &connector);
        assert!(report.token_ok && report.tools_ok, "{}", report.detail);
        let token = load_connector_token(store.as_ref(), &name)
            .unwrap()
            .unwrap();
        let mut link =
            ConnectorLink::open(&connector.manifest, &connector.dir, &token, SPAWN_TIMEOUT)
                .unwrap();
        let result = link.call(&tool, HashMap::new()).unwrap();
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("text content in tool result");
        assert!(
            text.contains("|token:sekret-1"),
            "token was not injected into the process env: {text}"
        );

        // disconnect revokes; doctor fails again.
        fx.pin_env();
        assert_eq!(disconnect_cmd(&name, true), 0);
        assert_eq!(load_connector_token(store.as_ref(), &name).unwrap(), None);
        let report = doctor_one(store.as_ref(), &connector);
        assert!(!report.token_ok && !report.tools_ok);

        // disconnecting twice is a no-op, not an error.
        fx.pin_env();
        assert_eq!(disconnect_cmd(&name, true), 0);
    }

    #[test]
    fn connect_none_flow_stores_nothing() {
        let fx = Fixture::new();
        let store = connector_store();
        fx.pin_env();
        assert_eq!(connect_cmd("stubby", None, true), 0);
        assert_eq!(
            load_connector_token(store.as_ref(), "stubby").unwrap(),
            None
        );
        // Doctor stays green for flow=none with no token.
        let connector = find_connector("stubby").unwrap();
        let report = doctor_one(store.as_ref(), &connector);
        assert!(report.token_ok && report.tools_ok);
    }

    #[test]
    fn connect_oauth2_is_honest_about_the_gap() {
        let fx = Fixture::new();
        let name = format!("oauthy-{}", Fixture::next_id());
        let tool = format!("{name}.echo");
        let manifest = manifest_for(&name, "oauth2", &tool, Some("allow"));
        Fixture::write_connector(&fx.dir, &name, &manifest, &tool, STUB);
        // Not implemented yet — must fail loudly, not pretend.
        fx.pin_env();
        assert_eq!(connect_cmd(&name, None, true), 1);
    }

    #[test]
    fn install_from_path_and_by_name() {
        let fx = Fixture::new();
        fx.pin_env();

        // By path, with config values.
        let rc = install_test(
            fx.dir.join("stubby").to_str().unwrap(),
            &["region=us", "retries=3"],
            true,
        );
        assert_eq!(rc, 0);
        let dest = fx.install_dir.join("stubby");
        assert!(dest.join("connector.toml").is_file());
        assert!(dest.join("connector").is_file());
        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(dest.join("config.json")).unwrap())
                .unwrap();
        assert_eq!(config["region"], json!("us"));
        assert_eq!(config["retries"], json!(3));
        // The installed executable kept its mode.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dest.join("connector"))
                .unwrap()
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0, "executable bit lost in copy");
        }

        // Reinstalling refuses instead of clobbering.
        assert_eq!(
            install_test(fx.dir.join("stubby").to_str().unwrap(), &[], true),
            1
        );

        // By discovered name.
        assert_eq!(install_test("allowy", &[], true), 0);
        assert!(fx
            .install_dir
            .join("allowy")
            .join("connector.toml")
            .is_file());

        // Unknown name and bad config fail before touching the fs.
        assert_eq!(install_test("ghost", &[], true), 1);
        assert_eq!(install_test("allowy", &["noequals"], true), 1);

        // The installed connectors are discoverable from the install dir.
        let (found, errors) = discover(&[fx.install_dir.as_path()]);
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<&str> = found.iter().map(|c| c.manifest.name.as_str()).collect();
        assert!(names.contains(&"stubby"));
        assert!(names.contains(&"allowy"));
    }

    #[test]
    fn run_refuses_ask_policy_tool() {
        let _fx = Fixture::new();
        // stubby.echo has no policy entry → defaults to Ask → the
        // non-interactive CLI must refuse.
        let rc = run_cmd("stubby", "stubby.echo", &["msg=hi"], true);
        assert_eq!(rc, 1);
        // Unknown tool is refused before policy is even consulted.
        let rc = run_cmd("stubby", "nope", &[], true);
        assert_eq!(rc, 1);
        // Unknown connector.
        let rc = run_cmd("ghost", "ghost.echo", &[], true);
        assert_eq!(rc, 1);
    }

    #[test]
    fn run_end_to_end_allow_tool() {
        let _fx = Fixture::new();
        // allowy.echo is Allow → full path: discovery → policy → keychain
        // (no token stored; empty is fine for the stub) → spawn → call.
        let rc = run_cmd("allowy", "allowy.echo", &["msg=hi"], true);
        assert_eq!(rc, 0);
    }

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn enable_attaches_connector_to_session() {
        let fx = Fixture::new();
        let session_dir = fx.make_session("sess-1");
        // stubby's auth flow is "none": no token needed.
        let rc = run(&s(&["enable", "stubby", "--session", "sess-1"]));
        assert_eq!(rc, 0);
        let attached = fx.attachments(&session_dir);
        let a = attached.connectors.get("stubby").expect("attached");
        assert!(a.policy.is_empty());
        assert!(a.enabled_at_unix_ms > 0);
        // Re-enabling is idempotent and reports success.
        let rc = run(&s(&["enable", "stubby", "--session", "sess-1"]));
        assert_eq!(rc, 0);
    }

    #[test]
    fn enable_rejects_unknown_session_and_bad_id() {
        let fx = Fixture::new();
        fx.pin_env();
        // No such session dir.
        let rc = run(&s(&["enable", "stubby", "--session", "nope"]));
        assert_eq!(rc, 1);
        // Path traversal is rejected before touching the filesystem.
        let rc = run(&s(&["enable", "stubby", "--session", "../evil"]));
        assert_eq!(rc, 1);
        let rc = run(&s(&["enable", "stubby", "--session", "a/b"]));
        assert_eq!(rc, 1);
        // Missing --session entirely.
        let rc = run(&s(&["enable", "stubby"]));
        assert_eq!(rc, 1);
    }

    #[test]
    fn enable_requires_connected_for_api_key() {
        let fx = Fixture::new();
        let session_dir = fx.make_session("sess-1");
        let (name, _tool) = fx.api_key_connector();
        // Not connected yet: refused, nothing written.
        let rc = run(&s(&["enable", &name, "--session", "sess-1"]));
        assert_eq!(rc, 1);
        assert!(fx.attachments(&session_dir).connectors.is_empty());
        // After connect, enable succeeds.
        fx.pin_env();
        let rc = run(&s(&["connect", &name, "--token", "sekret"]));
        assert_eq!(rc, 0);
        let rc = run(&s(&["enable", &name, "--session", "sess-1"]));
        assert_eq!(rc, 0);
        assert!(fx.attachments(&session_dir).connectors.contains_key(&name));
    }

    #[test]
    fn enable_policy_overrides_tighten_only() {
        let fx = Fixture::new();
        let session_dir = fx.make_session("sess-1");
        // allowy.echo defaults to Allow: tightening to deny is fine.
        let rc = run(&s(&[
            "enable",
            "allowy",
            "--session",
            "sess-1",
            "--policy",
            "allowy.echo=deny",
        ]));
        assert_eq!(rc, 0);
        let attached = fx.attachments(&session_dir);
        assert_eq!(
            attached.connectors["allowy"].policy["allowy.echo"],
            ApprovalPolicy::Deny
        );
        // stubby.echo defaults to Ask: loosening to allow is refused.
        let rc = run(&s(&[
            "enable",
            "stubby",
            "--session",
            "sess-1",
            "--policy",
            "stubby.echo=allow",
        ]));
        assert_eq!(rc, 1);
        // Unknown tool is refused.
        let rc = run(&s(&[
            "enable",
            "stubby",
            "--session",
            "sess-1",
            "--policy",
            "nope=deny",
        ]));
        assert_eq!(rc, 1);
        // Unknown connector is refused.
        let rc = run(&s(&["enable", "ghost", "--session", "sess-1"]));
        assert_eq!(rc, 1);
    }

    #[test]
    fn disable_detaches_and_is_idempotent() {
        let fx = Fixture::new();
        let session_dir = fx.make_session("sess-1");
        let rc = run(&s(&["enable", "stubby", "--session", "sess-1"]));
        assert_eq!(rc, 0);
        let rc = run(&s(&["disable", "stubby", "--session", "sess-1"]));
        assert_eq!(rc, 0);
        assert!(fx.attachments(&session_dir).connectors.is_empty());
        // Detaching again is a clean no-op.
        let rc = run(&s(&["disable", "stubby", "--session", "sess-1"]));
        assert_eq!(rc, 0);
        // Unknown session still fails.
        let rc = run(&s(&["disable", "stubby", "--session", "nope"]));
        assert_eq!(rc, 1);
    }

    #[test]
    fn new_scaffolds_a_discoverable_connector() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-conn-new-test-{}-{}",
            std::process::id(),
            Fixture::next_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rc = run(&s(&[
            "new",
            "newsvc",
            "--dir",
            dir.to_str().unwrap(),
            "--auth",
            "api-key",
            "--tool",
            "newsvc.search",
            "--tool",
            "newsvc.send",
            "--description",
            "a test service",
        ]));
        assert_eq!(rc, 0);
        let conn = dir.join("newsvc");
        let manifest_text = std::fs::read_to_string(conn.join("connector.toml")).unwrap();
        let manifest = parse_manifest(&manifest_text).expect("scaffolded manifest parses");
        assert_eq!(manifest.name, "newsvc");
        assert_eq!(manifest.auth_flow, AuthFlow::ApiKey);
        assert_eq!(manifest.provides, vec!["newsvc.search", "newsvc.send"]);
        assert!(manifest_text.contains("a test service"));
        // The stub is a real executable MCP server.
        let exe = conn.join("connector");
        assert!(exe.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "stub lost its executable bit");
        }
        // And the directory is discoverable as a connector.
        let (found, errors) = discover(&[dir.as_path()]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(found.iter().any(|c| c.manifest.name == "newsvc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_refuses_overwrite_and_bad_inputs() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-conn-new-test-{}-{}",
            std::process::id(),
            Fixture::next_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        assert_eq!(run(&s(&["new", "dupsvc", "--dir", d])), 0);
        // Second scaffold into the same name refuses instead of clobbering.
        assert_eq!(run(&s(&["new", "dupsvc", "--dir", d])), 1);
        // Unknown kind / auth / bad name / builtin all fail before writing.
        assert_eq!(run(&s(&["new", "x", "--dir", d, "--kind", "grpc"])), 1);
        assert_eq!(run(&s(&["new", "x", "--dir", d, "--auth", "token"])), 1);
        assert_eq!(run(&s(&["new", "Bad Name", "--dir", d])), 1);
        assert_eq!(run(&s(&["new", "x", "--dir", d, "--kind", "builtin"])), 1);
        assert!(!dir.join("x").exists(), "failed scaffold must not write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_http_variant_writes_config_not_executable() {
        let dir = std::env::temp_dir().join(format!(
            "supercli-conn-new-test-{}-{}",
            std::process::id(),
            Fixture::next_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rc = run(&s(&[
            "new",
            "websvc",
            "--dir",
            dir.to_str().unwrap(),
            "--kind",
            "mcp-http",
            "--json",
        ]));
        assert_eq!(rc, 0);
        let conn = dir.join("websvc");
        assert!(conn.join("connector.toml").is_file());
        assert!(conn.join("config.json").is_file());
        assert!(
            !conn.join("connector").exists(),
            "mcp-http has no stdio executable to scaffold"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    const MANIFEST_SCHEMA: &str = r#"
[connector]
name = "schemy"
version = "0.1.0"
display_name = "Schemy"
description = "config-schema test connector"
kind = "mcp-stdio"

[auth]
flow = "none"

[tools]
provides = ["schemy.echo"]

[config_schema]
region = { type = "string", title = "Region", default = "us" }
retries = { type = "integer", title = "Retries", default = 3 }
"#;

    const MANIFEST_HTTP: &str = r#"
[connector]
name = "httpy"
version = "0.1.0"
display_name = "Httpy"
description = "mcp-http test connector"
kind = "mcp-http"

[auth]
flow = "none"

[tools]
provides = ["httpy.echo"]

[policy]
"httpy.echo" = "allow"
"#;

    /// Minimal MCP-over-HTTP stub (plain JSON, no SSE), mirroring the
    /// one in supercli-connector's http tests. Serves `n` requests, then
    /// the listener drops.
    fn serve_http_stub(n: usize) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(n) {
                let mut stream = stream.unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                let body: Value = loop {
                    let r = stream.read(&mut buf).unwrap_or(0);
                    if r == 0 {
                        break Value::Null;
                    }
                    raw.extend_from_slice(&buf[..r]);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&raw[..end]).to_string();
                        let len: usize = headers
                            .lines()
                            .find(|l| l.to_lowercase().starts_with("content-length:"))
                            .and_then(|l| l.split(':').nth(1))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        while raw.len() < end + 4 + len {
                            let r = stream.read(&mut buf).unwrap_or(0);
                            if r == 0 {
                                break;
                            }
                            raw.extend_from_slice(&buf[..r]);
                        }
                        break serde_json::from_slice(&raw[end + 4..]).unwrap_or(Value::Null);
                    }
                };
                let id = body.get("id").cloned().unwrap_or(Value::Null);
                let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
                let result = match method {
                    "initialize" => {
                        json!({"protocolVersion": "2024-11-05", "capabilities": {}})
                    }
                    "tools/list" => json!({"tools": [
                        {"name": "httpy.echo", "description": "echo",
                         "inputSchema": {"type": "object"}}
                    ]}),
                    "tools/call" => {
                        json!({"content": [{"type": "text", "text": "http-echo-ok"}]})
                    }
                    _ => json!({}),
                };
                // Notifications carry no id and get an empty body back.
                let payload = if body.get("id").is_some() {
                    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
                } else {
                    String::new()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (url, handle)
    }

    #[test]
    fn keygen_pack_publish_registry_install_roundtrip() {
        let fx = Fixture::new();
        // A dashed name: the registry must read name/version from the
        // signed manifest, not by splitting the bundle file name.
        let name = format!("my-service-{}", Fixture::next_id());
        let tool = format!("{name}.echo");
        let manifest = manifest_for(&name, "none", &tool, None);
        Fixture::write_connector(&fx.dir, &name, &manifest, &tool, STUB);

        fx.pin_env();
        assert_eq!(keygen_cmd("testkey", true), 0);
        assert!(fx.keys_dir.join("testkey.key").is_file());
        assert!(fx.keys_dir.join("testkey.pub").is_file());
        // keygen refuses to overwrite an existing key id.
        fx.pin_env();
        assert_eq!(keygen_cmd("testkey", true), 1);

        let out = fx.dir.join("bundles-out");
        fx.pin_env();
        assert_eq!(
            pack_cmd(&name, Some(out.to_str().unwrap()), "testkey", true),
            0
        );
        let entries: Vec<_> = std::fs::read_dir(&out).unwrap().collect();
        assert_eq!(entries.len(), 2, "bundle plus .sig sidecar");

        let registry = fx.dir.join("registry");
        fx.pin_env();
        assert_eq!(
            publish_cmd(&name, registry.to_str().unwrap(), "testkey", true),
            0
        );

        // Unknown version fails; the published version installs.
        let reg_opts = |source: String| InstallOptions {
            source,
            config_pairs: vec![],
            registry: Some(registry.to_str().unwrap().to_string()),
            form: false,
            require_signature: false,
            pubkey: None,
            key_id: None,
        };
        fx.pin_env();
        assert_eq!(install_cmd(&reg_opts(format!("{name}@9.9.9")), true), 1);
        fx.pin_env();
        assert_eq!(install_cmd(&reg_opts(name.clone()), true), 0);
        assert!(fx.install_dir.join(&name).join("connector.toml").is_file());

        // A different publisher key is pinned out: refused.
        fx.pin_env();
        assert_eq!(keygen_cmd("otherkey", true), 0);
        fx.pin_env();
        assert_eq!(
            publish_cmd(&name, registry.to_str().unwrap(), "otherkey", true),
            1
        );

        // Tampering with the registry bundle breaks the digest check on
        // the next install.
        let _ = std::fs::remove_dir_all(fx.install_dir.join(&name));
        let bundle = std::fs::read_dir(registry.join("bundles"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .next()
            .unwrap();
        let mut bytes = std::fs::read(&bundle).unwrap();
        bytes.push(0xAA);
        std::fs::write(&bundle, &bytes).unwrap();
        fx.pin_env();
        assert_eq!(install_cmd(&reg_opts(name.clone()), true), 1);
    }

    #[test]
    fn install_from_bundle_file_verifies_signature() {
        let fx = Fixture::new();
        let name = format!("bundlesvc-{}", Fixture::next_id());
        let tool = format!("{name}.echo");
        let manifest = manifest_for(&name, "none", &tool, None);
        Fixture::write_connector(&fx.dir, &name, &manifest, &tool, STUB);
        fx.pin_env();
        assert_eq!(keygen_cmd("bkey", true), 0);
        fx.pin_env();
        assert_eq!(keygen_cmd("wrongkey", true), 0);
        let out = fx.dir.join("bout");
        fx.pin_env();
        assert_eq!(
            pack_cmd(&name, Some(out.to_str().unwrap()), "bkey", true),
            0
        );
        let bundle = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "supercli-connector"))
            .expect("packed bundle");
        let base_opts = || InstallOptions {
            source: bundle.to_str().unwrap().to_string(),
            config_pairs: vec![],
            registry: None,
            form: false,
            require_signature: false,
            pubkey: None,
            key_id: None,
        };

        // No trusted key at all: refused (fail closed).
        let mut opts = base_opts();
        opts.key_id = Some("no-such-key".to_string());
        fx.pin_env();
        assert_eq!(install_cmd(&opts, true), 1);

        // The wrong publisher's key: refused.
        let mut opts = base_opts();
        opts.key_id = Some("wrongkey".to_string());
        fx.pin_env();
        assert_eq!(install_cmd(&opts, true), 1);

        // The real publisher's public key file: installs.
        let mut opts = base_opts();
        opts.pubkey = Some(fx.keys_dir.join("bkey.pub").to_str().unwrap().to_string());
        fx.pin_env();
        assert_eq!(install_cmd(&opts, true), 0);
        assert!(fx.install_dir.join(&name).join("connector.toml").is_file());

        // --require-signature refuses loose directory installs.
        let dir_opts = InstallOptions {
            source: fx.dir.join("stubby").to_str().unwrap().to_string(),
            config_pairs: vec![],
            registry: None,
            form: false,
            require_signature: true,
            pubkey: None,
            key_id: None,
        };
        fx.pin_env();
        assert_eq!(install_cmd(&dir_opts, true), 1);
    }

    #[test]
    fn config_cmd_validates_against_schema() {
        let fx = Fixture::new();
        Fixture::write_connector(&fx.dir, "schemy", MANIFEST_SCHEMA, "schemy.echo", STUB);
        fx.pin_env();
        assert_eq!(
            install_test(fx.dir.join("schemy").to_str().unwrap(), &[], true),
            0
        );
        // Remove the source so discovery targets the installed copy.
        let _ = std::fs::remove_dir_all(fx.dir.join("schemy"));

        // Valid pairs are schema-coerced ("5" -> 5) and written.
        fx.pin_env();
        assert_eq!(
            config_cmd(
                "schemy",
                false,
                &["region=eu".to_string(), "retries=5".to_string()],
                true
            ),
            0
        );
        let read = || {
            let text =
                std::fs::read_to_string(fx.install_dir.join("schemy").join("config.json")).unwrap();
            serde_json::from_str::<Value>(&text).unwrap()
        };
        assert_eq!(read()["region"], json!("eu"));
        assert_eq!(read()["retries"], json!(5));

        // Unknown field and bad type are refused; the file is untouched.
        fx.pin_env();
        assert_eq!(
            config_cmd("schemy", false, &["bogus=1".to_string()], true),
            1
        );
        fx.pin_env();
        assert_eq!(
            config_cmd("schemy", false, &["retries=many".to_string()], true),
            1
        );
        assert_eq!(read()["retries"], json!(5));

        // View mode (no pairs) prints the current config.
        fx.pin_env();
        assert_eq!(config_cmd("schemy", false, &[], true), 0);
        // Unknown connector fails with a helpful list.
        fx.pin_env();
        assert_eq!(config_cmd("ghost", false, &[], true), 1);
    }

    #[test]
    fn audit_cmd_reads_session_log() {
        let fx = Fixture::new();
        let dir = fx.make_session("audit-sess");
        std::fs::write(
            dir.join("connectors-audit.jsonl"),
            "{\"ts\":\"2026-09-22T00:00:01Z\",\"session\":\"audit-sess\",\"connector\":\"c1\",\"tool\":\"c1.a\",\"policy\":\"allow\",\"ok\":true,\"ms\":12}\n\
             {\"ts\":\"2026-09-22T00:00:02Z\",\"session\":\"audit-sess\",\"connector\":\"c1\",\"tool\":\"c1.b\",\"policy\":\"ask\",\"ok\":false,\"error\":\"denied\"}\n",
        )
        .unwrap();
        let opts = |tool: Option<&str>| AuditOptions {
            session: "audit-sess".to_string(),
            tool: tool.map(str::to_string),
            limit: 50,
        };
        fx.pin_env();
        assert_eq!(audit_cmd(opts(None), true), 0);
        fx.pin_env();
        assert_eq!(audit_cmd(opts(Some("c1.b")), true), 0);
        fx.pin_env();
        assert_eq!(audit_cmd(opts(Some("c1.zzz")), true), 0);
        // Unknown session fails with the fix spelled out.
        fx.pin_env();
        assert_eq!(
            audit_cmd(
                AuditOptions {
                    session: "nope".to_string(),
                    tool: None,
                    limit: 50,
                },
                true
            ),
            1
        );
    }

    #[test]
    fn sync_installs_from_registry_and_verifies() {
        let fx = Fixture::new();
        fx.pin_env();
        // Something installed locally so the verify pass has work.
        assert_eq!(
            install_test(fx.dir.join("stubby").to_str().unwrap(), &[], true),
            0
        );

        // Publish a registry connector.
        let name = format!("syncsvc-{}", Fixture::next_id());
        let tool = format!("{name}.echo");
        let manifest = manifest_for(&name, "none", &tool, None);
        Fixture::write_connector(&fx.dir, &name, &manifest, &tool, STUB);
        let registry = fx.dir.join("registry");
        fx.pin_env();
        assert_eq!(keygen_cmd("synckey", true), 0);
        fx.pin_env();
        assert_eq!(
            publish_cmd(&name, registry.to_str().unwrap(), "synckey", true),
            0
        );

        // sync installs it (digest + signature verified) and verifies the
        // rest without spawning anything.
        fx.pin_env();
        assert_eq!(sync_cmd(Some(registry.to_str().unwrap()), true), 0);
        assert!(fx.install_dir.join(&name).join("connector.toml").is_file());

        // Second sync: already at latest, still green.
        fx.pin_env();
        assert_eq!(sync_cmd(Some(registry.to_str().unwrap()), true), 0);

        // A broken registry path fails instead of silently no-op'ing.
        fx.pin_env();
        assert_eq!(
            sync_cmd(
                Some(fx.dir.join("no-such-registry").to_str().unwrap()),
                true
            ),
            1
        );
    }

    #[test]
    fn sync_update_is_atomic_and_preserves_config() {
        let fx = Fixture::new();
        let name = format!("updsync-{}", Fixture::next_id());
        let tool = format!("{name}.echo");
        let registry = fx.dir.join("registry");

        let write_version = |version: &str, bundle_config: &str| {
            let manifest = format!(
                "[connector]\nname = \"{name}\"\nversion = \"{version}\"\n\
                 display_name = \"{name}\"\ndescription = \"test\"\nkind = \"mcp-stdio\"\n\n\
                 [auth]\nflow = \"none\"\n\n[tools]\nprovides = [\"{tool}\"]\n\n\
                 [policy]\n\"{tool}\" = \"allow\"\n"
            );
            Fixture::write_connector(&fx.dir, &name, &manifest, &tool, STUB);
            std::fs::write(fx.dir.join(&name).join("config.json"), bundle_config).unwrap();
        };
        // publish resolves the name through discovery; move the installed
        // copy aside so the scaffold (the new version) is what packs.
        let publish_next = |version: &str| {
            let installed = fx.install_dir.join(&name);
            let aside = fx.install_dir.join(format!(".aside-{name}"));
            let had_installed = installed.is_dir();
            if had_installed {
                std::fs::rename(&installed, &aside).unwrap();
            }
            fx.pin_env();
            let rc = publish_cmd(&name, registry.to_str().unwrap(), "updkey", true);
            if had_installed {
                std::fs::rename(&aside, &installed).unwrap();
            }
            assert_eq!(rc, 0, "publish {version}");
        };

        fx.pin_env();
        assert_eq!(keygen_cmd("updkey", true), 0);
        write_version("0.1.0", r#"{"custom":"bundle-v1"}"#);
        publish_next("0.1.0");

        // v1 installs.
        fx.pin_env();
        assert_eq!(sync_cmd(Some(registry.to_str().unwrap()), true), 0);
        let installed_version = || {
            let text =
                std::fs::read_to_string(fx.install_dir.join(&name).join("connector.toml")).unwrap();
            text.lines()
                .find(|l| l.starts_with("version"))
                .unwrap()
                .to_string()
        };
        assert!(
            installed_version().contains("0.1.0"),
            "{:?}",
            installed_version()
        );
        let read_config =
            || std::fs::read_to_string(fx.install_dir.join(&name).join("config.json")).unwrap();

        // The user customizes config.json; the bundle default must not win.
        std::fs::write(
            fx.install_dir.join(&name).join("config.json"),
            r#"{"custom":"mine"}"#,
        )
        .unwrap();

        // v2 update: version moves, local config survives.
        write_version("0.2.0", r#"{"custom":"bundle-v2"}"#);
        publish_next("0.2.0");
        fx.pin_env();
        assert_eq!(sync_cmd(Some(registry.to_str().unwrap()), true), 0);
        assert!(
            installed_version().contains("0.2.0"),
            "{:?}",
            installed_version()
        );
        assert!(read_config().contains("\"mine\""), "{}", read_config());

        // Publish v3, then corrupt the registry bundle: the update must
        // fail (digest mismatch) and the v0.2.0 install — with the user's
        // config — must survive untouched.
        write_version("0.3.0", r#"{"custom":"bundle-v3"}"#);
        publish_next("0.3.0");
        let bundle_path = registry.join(format!("bundles/{name}-0.3.0.supercli-connector"));
        let mut bytes = std::fs::read(&bundle_path).unwrap();
        bytes.extend_from_slice(b"tampered");
        std::fs::write(&bundle_path, &bytes).unwrap();

        fx.pin_env();
        assert_eq!(sync_cmd(Some(registry.to_str().unwrap()), true), 1);
        assert!(
            installed_version().contains("0.2.0"),
            "{:?}",
            installed_version()
        );
        assert!(read_config().contains("\"mine\""), "{}", read_config());
        // No staging or backup debris left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&fx.install_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".sync-staging") || n.contains("backup"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn http_connector_doctor_and_run() {
        let fx = Fixture::new();
        // doctor: initialize + notify + tools/list = 3 requests;
        // run: the same 3 + tools/call = 4.
        let (url, handle) = serve_http_stub(7);
        Fixture::write_connector(&fx.dir, "httpy", MANIFEST_HTTP, "httpy.echo", STUB);
        fx.pin_env();
        std::fs::write(
            fx.dir.join("httpy").join("config.json"),
            format!("{{\"mcp_url\": {url:?}}}"),
        )
        .unwrap();

        let store = connector_store();
        fx.pin_env();
        let connector = find_connector("httpy").expect("http fixture");
        let report = doctor_one(store.as_ref(), &connector);
        assert!(report.transport_ok, "{}", report.detail);
        assert!(report.tools_ok, "{}", report.detail);
        assert_eq!(report.tool_count, 1);

        fx.pin_env();
        assert_eq!(run_cmd("httpy", "httpy.echo", &[], true), 0);
        handle.join().unwrap();
    }

    #[test]
    fn dispatch_reaches_new_verbs() {
        let fx = Fixture::new();
        fx.pin_env();
        assert_eq!(run(&s(&["keygen", "--key-id", "dispatchkey", "--json"])), 0);
        fx.pin_env();
        assert!(fx.keys_dir.join("dispatchkey.pub").is_file());
        // pack of an unknown connector fails through the real dispatch.
        fx.pin_env();
        assert_eq!(run(&s(&["pack", "ghost", "--key-id", "dispatchkey"])), 1);
        // sync with nothing installed and no registry: verify pass only.
        fx.pin_env();
        assert_eq!(run(&s(&["sync", "--json"])), 0);
    }
    #[test]
    fn disconnect_detaches_from_all_sessions() {
        let fx = Fixture::new();
        let dir_a = fx.make_session("sess-a");
        let dir_b = fx.make_session("sess-b");
        let (name, _tool) = fx.api_key_connector();
        fx.pin_env();
        assert_eq!(run(&s(&["connect", &name, "--token", "sekret"])), 0);
        assert_eq!(run(&s(&["enable", &name, "--session", "sess-a"])), 0);
        assert_eq!(run(&s(&["enable", &name, "--session", "sess-b"])), 0);
        assert!(fx.attachments(&dir_a).connectors.contains_key(&name));
        // Disconnect revokes the token AND detaches everywhere.
        assert_eq!(run(&s(&["disconnect", &name, "--json"])), 0);
        assert!(fx.attachments(&dir_a).connectors.is_empty());
        assert!(fx.attachments(&dir_b).connectors.is_empty());
    }
}
