//! Declarative hook registration: `hooks.toml`.
//!
//! Two files are loaded and merged:
//!
//! - global: `~/.supercli/hooks.toml`
//! - project: `.supercli/hooks.toml` (project = cwd at invocation)
//!
//! Merge rule: global handlers run before project handlers at equal
//! priority. Handler fields: `name` (required; becomes the audit actor
//! `hook:<name>`), `priority` (default 100, lower runs first),
//! `timeout_ms` (default 2000), and exactly one of `command` (argv array)
//! or `webhook` (URL, localhost only).
//!
//! Fail-closed at registration: a `command` not found on PATH, or a
//! `webhook` not resolving to localhost, disables the handler with a
//! config error instead of failing at 2 a.m.

use crate::{DocEvent, DocType, DEFAULT_PRIORITY, DEFAULT_TIMEOUT_MS};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One handler entry as declared in `hooks.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct HandlerDecl {
    pub name: String,
    #[serde(default = "default_priority")]
    pub priority: u32,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// argv array; `command[0]` is the program.
    pub command: Option<Vec<String>>,
    /// Webhook URL — localhost only (`127.0.0.1`, `::1`, `localhost`).
    pub webhook: Option<String>,
}

fn default_priority() -> u32 {
    DEFAULT_PRIORITY
}
fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// A handler that passed validation and is ready to run.
#[derive(Debug, Clone)]
pub struct HookHandler {
    pub name: String,
    pub entity: DocType,
    pub event: DocEvent,
    pub priority: u32,
    pub timeout_ms: u64,
    pub target: HandlerTarget,
    /// `true` when loaded from the global file (runs before project
    /// handlers at equal priority).
    pub global: bool,
    /// Declaration order within its file (final tiebreaker).
    pub order: usize,
}

#[derive(Debug, Clone)]
pub enum HandlerTarget {
    Command(Vec<String>),
    Webhook(String),
}

/// A handler that failed validation: disabled, with the reason.
#[derive(Debug, Clone)]
pub struct DisabledHandler {
    pub name: String,
    pub entity: String,
    pub event: String,
    pub reason: String,
    pub global: bool,
}

#[derive(Debug, Default)]
pub struct HookRegistry {
    /// (entity, event) -> handlers in effective execution order.
    handlers: HashMap<(DocType, DocEvent), Vec<HookHandler>>,
    pub disabled: Vec<DisabledHandler>,
}

impl HookRegistry {
    /// Load and merge the global and project `hooks.toml` files.
    /// Missing files are fine (empty registry); parse errors fail the
    /// whole load (fail-closed: a broken config must not silently mean
    /// "no hooks").
    pub fn load(global_path: &Path, project_path: &Path) -> Result<Self, String> {
        let mut reg = HookRegistry::default();
        if global_path.exists() {
            let order_base = 0;
            reg.load_file(global_path, true, order_base)?;
        }
        if project_path.exists() {
            let order_base = reg.handler_count();
            reg.load_file(project_path, false, order_base)?;
        }
        reg.sort_all();
        Ok(reg)
    }

    fn handler_count(&self) -> usize {
        self.handlers.values().map(Vec::len).sum()
    }

    fn load_file(&mut self, path: &Path, global: bool, order_base: usize) -> Result<(), String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let doc: toml::Value =
            toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
        let doc_events = doc
            .get("doc_events")
            .and_then(|v| v.as_table())
            .ok_or_else(|| format!("{}: missing [doc_events] table", path.display()))?;

        let mut order = order_base;
        for (entity_name, events) in doc_events {
            let entity = DocType::parse(entity_name).ok_or_else(|| {
                format!(
                    "{}: unknown entity {entity_name:?} (expected one of Session, Turn, ToolCall, Approval, Grant, Schedule, Job, Connector, Device, FileWrite, Idea)",
                    path.display()
                )
            })?;
            let events = events.as_table().ok_or_else(|| {
                format!(
                    "{}: [doc_events.{entity_name}] must be a table of event -> handlers",
                    path.display()
                )
            })?;
            for (event_name, handlers) in events {
                let event = DocEvent::parse(event_name).ok_or_else(|| {
                    format!(
                        "{}: unknown event {event_name:?} for entity {entity_name}",
                        path.display()
                    )
                })?;
                let handlers = handlers.as_array().ok_or_else(|| {
                    format!(
                        "{}: {entity_name}.{event_name} must be a list of handlers",
                        path.display()
                    )
                })?;
                for h in handlers {
                    let decl: HandlerDecl = h.clone().try_into().map_err(|e| {
                        format!(
                            "{}: bad handler for {entity_name}.{event_name}: {e}",
                            path.display()
                        )
                    })?;
                    order += 1;
                    self.add_decl(entity, event, decl, global, order, path);
                }
            }
        }
        Ok(())
    }

    fn add_decl(
        &mut self,
        entity: DocType,
        event: DocEvent,
        decl: HandlerDecl,
        global: bool,
        order: usize,
        path: &Path,
    ) {
        let entity_s = entity.as_str().to_string();
        let event_s = event.as_str().to_string();
        let disable = |reason: String| DisabledHandler {
            name: decl.name.clone(),
            entity: entity_s.clone(),
            event: event_s.clone(),
            reason,
            global,
        };

        if decl.name.trim().is_empty() {
            self.disabled.push(disable(format!(
                "{}: handler name must not be empty",
                path.display()
            )));
            return;
        }
        let target = match (decl.command, decl.webhook) {
            (Some(cmd), None) => {
                if cmd.is_empty() {
                    self.disabled.push(disable(format!(
                        "{}: handler {:?}: command must be a non-empty argv array",
                        path.display(),
                        decl.name
                    )));
                    return;
                }
                // Fail closed at registration: the program must exist now.
                if !command_on_path(&cmd[0]) {
                    self.disabled.push(disable(format!(
                        "{}: handler {:?}: command {:?} not found on PATH",
                        path.display(),
                        decl.name,
                        cmd[0]
                    )));
                    return;
                }
                HandlerTarget::Command(cmd)
            }
            (None, Some(url)) => match validate_localhost_url(&url) {
                Ok(()) => HandlerTarget::Webhook(url),
                Err(reason) => {
                    self.disabled.push(disable(format!(
                        "{}: handler {:?}: webhook {url:?}: {reason}",
                        path.display(),
                        decl.name
                    )));
                    return;
                }
            },
            (Some(_), Some(_)) => {
                self.disabled.push(disable(format!(
                    "{}: handler {:?}: specify exactly one of command / webhook",
                    path.display(),
                    decl.name
                )));
                return;
            }
            (None, None) => {
                self.disabled.push(disable(format!(
                    "{}: handler {:?}: specify command or webhook",
                    path.display(),
                    decl.name
                )));
                return;
            }
        };

        self.handlers
            .entry((entity, event))
            .or_default()
            .push(HookHandler {
                name: decl.name,
                entity,
                event,
                priority: decl.priority,
                timeout_ms: decl.timeout_ms,
                target,
                global,
                order,
            });
    }

    /// Deterministic execution order: ascending priority, then global
    /// before project, then declaration order.
    fn sort_all(&mut self) {
        for handlers in self.handlers.values_mut() {
            handlers.sort_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then(b.global.cmp(&a.global))
                    .then(a.order.cmp(&b.order))
            });
        }
    }

    /// Handlers for one (entity, event) in effective execution order.
    /// Returns an empty slice when nothing is registered.
    pub fn handlers_for(&self, entity: DocType, event: DocEvent) -> &[HookHandler] {
        self.handlers
            .get(&(entity, event))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// All registered (entity, event) pairs, sorted for `hooks list`.
    pub fn all_pairs(&self) -> Vec<(DocType, DocEvent, &[HookHandler])> {
        let mut pairs: Vec<_> = self
            .handlers
            .iter()
            .map(|((e, ev), hs)| (*e, *ev, hs.as_slice()))
            .collect();
        pairs.sort_by(|a, b| {
            a.0.as_str()
                .cmp(b.0.as_str())
                .then(a.1.as_str().cmp(b.1.as_str()))
        });
        pairs
    }
}

/// Resolve the conventional global / project hook file paths.
pub fn default_paths() -> (PathBuf, PathBuf) {
    let global = home_dir()
        .map(|h| h.join(".supercli").join("hooks.toml"))
        .unwrap_or_else(|| PathBuf::from(".supercli-hooks-global.toml"));
    let project = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".supercli")
        .join("hooks.toml");
    (global, project)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// `true` if `program` is an absolute path that exists, or resolves via
/// `PATH`. No shell involved.
fn command_on_path(program: &str) -> bool {
    let p = Path::new(program);
    if p.is_absolute() {
        return p.is_file();
    }
    if program.contains('/') || program.contains('\\') {
        return p.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Webhooks may only target localhost. Anything else is a config error
/// (deliberate exfiltration guard, design §5).
fn validate_localhost_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .ok_or_else(|| "URL must start with http:// or https://".to_string())?;
    // Strip optional userinfo, then the path; what remains is host[:port].
    let after_userinfo = rest.rsplit('@').next().unwrap_or(rest);
    let host_port = after_userinfo.split('/').next().unwrap_or(after_userinfo);
    // Bracketed IPv6 literal: the host is inside the brackets (a naive
    // split on ':' would shred "::1").
    let host = if let Some(inside) = host_port.strip_prefix('[') {
        inside.split(']').next().unwrap_or(inside)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    match host {
        "127.0.0.1" | "::1" | "localhost" => Ok(()),
        _ => Err("webhook host must be 127.0.0.1, ::1, or localhost".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_toml(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("supercli-events-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn localhost_webhook_validation() {
        assert!(validate_localhost_url("http://127.0.0.1:8787/hook").is_ok());
        assert!(validate_localhost_url("http://localhost:8787/hook").is_ok());
        assert!(validate_localhost_url("http://[::1]:8787/hook").is_ok());
        assert!(validate_localhost_url("https://127.0.0.1/hook").is_ok());
        assert!(validate_localhost_url("http://127.0.0.1@evil.com/").is_err());
        assert!(validate_localhost_url("http://example.com/hook").is_err());
        assert!(validate_localhost_url("http://10.0.0.1/hook").is_err());
        assert!(validate_localhost_url("ftp://127.0.0.1/hook").is_err());
    }

    #[test]
    fn missing_command_disables_handler() {
        let d = tmpdir("missing-cmd");
        let global = write_toml(
            &d,
            "hooks.toml",
            r#"
[doc_events.ToolCall]
before_execute = [
  { name = "ghost", command = ["definitely-not-a-real-binary-xyz"], priority = 10 },
]
"#,
        );
        let reg = HookRegistry::load(&global, &d.join("nope.toml")).unwrap();
        assert!(reg
            .handlers_for(DocType::ToolCall, DocEvent::BeforeExecute)
            .is_empty());
        assert_eq!(reg.disabled.len(), 1);
        assert!(reg.disabled[0].reason.contains("not found on PATH"));
    }

    #[test]
    fn merge_order_global_before_project_at_equal_priority() {
        let d = tmpdir("merge-order");
        let global = write_toml(
            &d,
            "global.toml",
            r#"
[doc_events.Session]
on_update = [
  { name = "g1", command = ["true"], priority = 100 },
  { name = "g2", command = ["true"], priority = 50 },
]
"#,
        );
        let project = write_toml(
            &d,
            "project.toml",
            r#"
[doc_events.Session]
on_update = [
  { name = "p1", command = ["true"], priority = 50 },
  { name = "p2", command = ["true"], priority = 200 },
]
"#,
        );
        let reg = HookRegistry::load(&global, &project).unwrap();
        let names: Vec<_> = reg
            .handlers_for(DocType::Session, DocEvent::OnUpdate)
            .iter()
            .map(|h| h.name.as_str())
            .collect();
        // priority 50: global g2 before project p1; then priority 100 g1; then 200 p2.
        assert_eq!(names, vec!["g2", "p1", "g1", "p2"]);
    }

    #[test]
    fn unknown_entity_is_a_load_error() {
        let d = tmpdir("unknown-entity");
        let global = write_toml(&d, "hooks.toml", "[doc_events.Nope]\non_update = []\n");
        assert!(HookRegistry::load(&global, &d.join("nope")).is_err());
    }

    #[test]
    fn missing_files_give_empty_registry() {
        let d = tmpdir("missing-files");
        let reg = HookRegistry::load(&d.join("a.toml"), &d.join("b.toml")).unwrap();
        assert!(reg
            .handlers_for(DocType::Session, DocEvent::OnUpdate)
            .is_empty());
        assert!(reg.disabled.is_empty());
    }
}

impl HookRegistry {
    /// `true` if at least one enabled handler is registered for the entity/event.
    /// Used by the emit fast path to avoid dispatcher setup when nothing is registered.
    pub fn has_handlers(&self, entity: DocType, event: DocEvent) -> bool {
        self.handlers
            .get(&(entity, event))
            .map(|hs| !hs.is_empty())
            .unwrap_or(false)
    }
}
