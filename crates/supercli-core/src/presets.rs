//! Quick-launch preset model + selection rules.
//!
//! Moved from the Dart client (`clients/supercli-app/lib/presets.dart`) per
//! Amein's rule: one implementation, in Rust. The Dart client keeps only UI
//! bindings via `supercli-client-ffi`.
//!
//! The preset list is FLAT and user-ordered (no per-CLI sections). Tool
//! identification is catalog-backed: every lookup goes through the generated
//! runtime catalog. Catalog-backed lookups are native-host only; the portable
//! Controller core keeps the preset model, ordering, and grouping logic.

#[cfg(feature = "native-host")]
use std::collections::HashMap;

#[cfg(feature = "native-host")]
use crate::runtime_catalog::{RuntimeCatalog, RuntimeDescriptor};

/// Blank-terminal pseudo-preset id.
pub const NEW_TERMINAL_ID: &str = "__new_terminal__";

/// A quick-launch preset: a named command the user can launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub command: String,
    pub enabled: bool,
    pub quick_launch: bool,
}

impl Preset {
    /// The blank-terminal pseudo-preset.
    pub fn new_terminal() -> Self {
        Self {
            id: NEW_TERMINAL_ID.to_string(),
            label: "Terminal".to_string(),
            command: String::new(),
            enabled: true,
            quick_launch: false,
        }
    }

    pub fn is_new_terminal(&self) -> bool {
        self.id == NEW_TERMINAL_ID
    }

    /// Sanitized copy: `quick_launch` only when the command is non-empty.
    pub fn sanitized(&self) -> Self {
        Self {
            id: self.id.clone(),
            label: self.label.clone(),
            command: self.command.clone(),
            enabled: self.enabled,
            quick_launch: self.quick_launch && !self.command.trim().is_empty(),
        }
    }
}

/// Catalog-backed tool identification for quick presets.
///
/// A tool is valid when the runtime exists in the catalog and supports quick
/// launch. Mirrors Dart's `QuickPresetTool`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuickPresetTool {
    /// The runtime's legacy slug (compatibility identity).
    raw_value: String,
}

impl QuickPresetTool {
    /// Creates a tool if the runtime exists and supports quick launch.
    ///
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn new(catalog: &RuntimeCatalog, raw_value: &str) -> Option<Self> {
        let runtime = catalog.by_legacy_slug(raw_value)?;
        if !runtime.supports_quick_launch {
            return None;
        }
        Some(Self {
            raw_value: runtime.legacy_slug.clone(),
        })
    }

    /// Creates a tool without validation (for source compatibility).
    pub fn unchecked(raw_value: &str) -> Self {
        Self {
            raw_value: raw_value.to_string(),
        }
    }

    /// All quick-launchable tools in the catalog.
    ///
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn all_cases(catalog: &RuntimeCatalog) -> Vec<Self> {
        catalog
            .descriptors()
            .iter()
            .filter(|r| r.supports_quick_launch)
            .map(|r| Self {
                raw_value: r.legacy_slug.clone(),
            })
            .collect()
    }

    pub fn id(&self) -> &str {
        &self.raw_value
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn metadata<'a>(&self, catalog: &'a RuntimeCatalog) -> Option<&'a RuntimeDescriptor> {
        catalog.by_legacy_slug(&self.raw_value)
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn display_name(&self, catalog: &RuntimeCatalog) -> String {
        match self.metadata(catalog) {
            Some(m) => capitalize(&m.label),
            None => self.raw_value.clone(),
        }
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn icon_key<'a>(&self, catalog: &'a RuntimeCatalog) -> &'a str {
        self.metadata(catalog)
            .map(|m| m.display.icon.as_str())
            .unwrap_or("agent")
    }

    /// Detects the tool from a command string.
    ///
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn detect(catalog: &RuntimeCatalog, command: &str) -> Option<Self> {
        let runtime = descriptor_for_command(catalog, command)?;
        if !runtime.supports_quick_launch {
            return None;
        }
        Some(Self {
            raw_value: runtime.legacy_slug.clone(),
        })
    }
}

/// Catalog-backed setup tool (includes non-quick-launchable tools).
///
/// Mirrors Dart's `SetupTool`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetupTool {
    raw_value: String,
}

impl SetupTool {
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn new(catalog: &RuntimeCatalog, raw_value: &str) -> Option<Self> {
        let runtime = catalog.by_legacy_slug(raw_value)?;
        Some(Self {
            raw_value: runtime.legacy_slug.clone(),
        })
    }

    pub fn unchecked(raw_value: &str) -> Self {
        Self {
            raw_value: raw_value.to_string(),
        }
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn all_cases(catalog: &RuntimeCatalog) -> Vec<Self> {
        catalog
            .descriptors()
            .iter()
            .map(|r| Self {
                raw_value: r.legacy_slug.clone(),
            })
            .collect()
    }

    pub fn id(&self) -> &str {
        &self.raw_value
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn metadata<'a>(&self, catalog: &'a RuntimeCatalog) -> Option<&'a RuntimeDescriptor> {
        catalog.by_legacy_slug(&self.raw_value)
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn display_name(&self, catalog: &RuntimeCatalog) -> String {
        match self.metadata(catalog) {
            Some(m) => capitalize(&m.label),
            None => self.raw_value.clone(),
        }
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn command_names(&self, catalog: &RuntimeCatalog) -> Vec<String> {
        match self.metadata(catalog) {
            Some(m) => m.detection.command_aliases.clone(),
            None => vec![self.raw_value.clone()],
        }
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn command_name(&self, catalog: &RuntimeCatalog) -> String {
        self.metadata(catalog)
            .and_then(|m| m.detection.command_aliases.first().cloned())
            .unwrap_or_else(|| self.raw_value.clone())
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn default_preset_command(&self, catalog: &RuntimeCatalog) -> String {
        self.metadata(catalog)
            .and_then(|m| m.suggested_presets.first().map(|p| p.command.clone()))
            .unwrap_or_else(|| self.command_name(catalog))
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn quick_preset_tool(&self, catalog: &RuntimeCatalog) -> Option<QuickPresetTool> {
        QuickPresetTool::new(catalog, &self.raw_value)
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn is_favorite_capable(&self, catalog: &RuntimeCatalog) -> bool {
        self.metadata(catalog)
            .is_some_and(|m| m.supports_quick_launch)
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn install_command<'a>(&self, catalog: &'a RuntimeCatalog) -> Option<&'a str> {
        self.metadata(catalog)?.install.as_ref()?.command.as_deref()
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn website_url<'a>(&self, catalog: &'a RuntimeCatalog) -> Option<&'a str> {
        Some(&self.metadata(catalog)?.install.as_ref()?.official_url)
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn uses_lifecycle_hooks(&self, catalog: &RuntimeCatalog) -> bool {
        self.metadata(catalog).is_some_and(|m| {
            m.capabilities
                .contains(&crate::runtime_catalog::RuntimeCapability::LifecycleHooks)
        })
    }

    /// Resolves a command to the CLI it launches.
    ///
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn detect(catalog: &RuntimeCatalog, command: &str) -> Option<Self> {
        let runtime = descriptor_for_command(catalog, command)?;
        Some(Self {
            raw_value: runtime.legacy_slug.clone(),
        })
    }
}

/// Resolves a command string to its runtime descriptor.
///
/// Mirrors the Dart `SupercliRuntimeCatalog.runtime(command:)` logic: take
/// the first whitespace-separated token, strip surrounding quotes, take the
/// basename, lowercase it, then match against command and process aliases.
///
/// Native-host only: needs the generated runtime catalog.
#[cfg(feature = "native-host")]
pub fn descriptor_for_command<'a>(
    catalog: &'a RuntimeCatalog,
    command: &str,
) -> Option<&'a RuntimeDescriptor> {
    let token = command.split_whitespace().next()?;
    let unquoted = token.trim_matches(|c| c == '\'' || c == '"');
    let binary = unquoted.rsplit('/').next().unwrap_or("").to_lowercase();
    if binary.is_empty() {
        return None;
    }
    // `by_executable_alias` checks command_aliases then process_aliases,
    // case-insensitively — matching the Dart lookup semantics.
    catalog.by_executable_alias(&binary)
}

/// Usage statistics for a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUsageStats {
    pub session_count: u64,
    pub recent_count: u64,
    /// Milliseconds since epoch of last use, if any.
    pub last_used_ms: Option<u64>,
}

impl ToolUsageStats {
    pub const NONE: Self = Self {
        session_count: 0,
        recent_count: 0,
        last_used_ms: None,
    };

    pub fn has_any(&self) -> bool {
        self.session_count > 0
    }

    /// Ordering: recent activity beats lifetime volume.
    pub fn more_used(a: &Self, b: &Self) -> bool {
        if a.recent_count != b.recent_count {
            return a.recent_count > b.recent_count;
        }
        if a.session_count != b.session_count {
            return a.session_count > b.session_count;
        }
        a.last_used_ms.unwrap_or(0) > b.last_used_ms.unwrap_or(0)
    }

    /// Human usage summary, e.g. "342 sessions · used today".
    /// `now_ms` is the current time in milliseconds since epoch.
    pub fn summary(&self, now_ms: u64) -> Option<String> {
        if self.session_count == 0 {
            return None;
        }
        let sessions = if self.session_count == 1 {
            "1 session".to_string()
        } else {
            format!("{} sessions", self.session_count)
        };
        let last = self.last_used_ms?;
        let days = days_between(last, now_ms);
        let recency = match days {
            0 => "used today".to_string(),
            1 => "used yesterday".to_string(),
            d if d <= 60 => format!("used {d} days ago"),
            _ => return Some(sessions),
        };
        Some(format!("{sessions} · {recency}"))
    }
}

fn days_between(earlier_ms: u64, later_ms: u64) -> u64 {
    const DAY_MS: u64 = 86_400_000;
    later_ms.saturating_sub(earlier_ms) / DAY_MS
}

/// Install status for a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInstallStatus {
    pub tool: SetupTool,
    pub path: Option<String>,
    pub usage: ToolUsageStats,
}

impl ToolInstallStatus {
    pub fn installed(&self) -> bool {
        self.path.is_some()
    }
}

/// Scan report for all tools.
#[derive(Debug, Clone)]
pub struct ToolScanReport {
    pub statuses: Vec<ToolInstallStatus>,
}

impl ToolScanReport {
    pub fn installed_statuses(&self) -> Vec<&ToolInstallStatus> {
        self.statuses.iter().filter(|s| s.installed()).collect()
    }

    pub fn missing_statuses(&self) -> Vec<&ToolInstallStatus> {
        self.statuses.iter().filter(|s| !s.installed()).collect()
    }

    pub fn any_ai_installed(&self) -> bool {
        self.statuses.iter().any(|s| s.installed())
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn installed_quick_tools(&self, catalog: &RuntimeCatalog) -> Vec<QuickPresetTool> {
        self.installed_statuses()
            .into_iter()
            .filter_map(|s| s.tool.quick_preset_tool(catalog))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn status_for(&self, tool: &SetupTool) -> Option<&ToolInstallStatus> {
        self.statuses.iter().find(|s| s.tool == *tool)
    }

    /// The installed CLI with the clearest usage lead (needs >= 3 sessions).
    pub fn most_used_tool(&self) -> Option<&SetupTool> {
        let mut ranked: Vec<&ToolInstallStatus> = self
            .installed_statuses()
            .into_iter()
            .filter(|s| s.usage.session_count >= 3)
            .collect();
        ranked.sort_by(|a, b| {
            if ToolUsageStats::more_used(&a.usage, &b.usage) {
                std::cmp::Ordering::Less
            } else if ToolUsageStats::more_used(&b.usage, &a.usage) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        ranked.first().map(|s| &s.tool)
    }

    /// Installed CLIs ordered most-used first (stable for ties).
    pub fn usage_ordered_installed_tools(&self) -> Vec<&SetupTool> {
        let mut indexed: Vec<(usize, &ToolInstallStatus)> =
            self.installed_statuses().into_iter().enumerate().collect();
        indexed.sort_by(|(ia, a), (ib, b)| {
            if a.usage != b.usage {
                if ToolUsageStats::more_used(&a.usage, &b.usage) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            } else {
                ia.cmp(ib)
            }
        });
        indexed.into_iter().map(|(_, s)| &s.tool).collect()
    }
}

/// A group of presets sharing one CLI identity, in flat-list order.
#[derive(Debug, Clone)]
pub struct QuickPresetGroup {
    pub cli: Option<SetupTool>,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub presets: Vec<Preset>,
}

impl QuickPresetGroup {
    pub fn id(&self) -> String {
        if let Some(cli) = &self.cli {
            return cli.id().to_string();
        }
        if let Some(app_id) = &self.app_id {
            return app_id.clone();
        }
        self.presets
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default()
    }

    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn display_name(&self, catalog: &RuntimeCatalog) -> String {
        if let Some(cli) = &self.cli {
            return cli.display_name(catalog);
        }
        if let Some(name) = &self.app_name {
            return name.clone();
        }
        self.presets
            .first()
            .map(|p| p.label.clone())
            .unwrap_or_default()
    }
}

/// One quick-access chip per agent or App, with its command variants in order.
///
/// `is_plugin_command` classifies a command as a plugin (from the Host's App
/// catalog); `app_for_head` resolves an executable basename to an app
/// `(id, name)` pair.
#[cfg(feature = "native-host")]
type PresetIdentity = (Option<SetupTool>, Option<(String, String)>);

/// Native-host only: needs the generated runtime catalog.
#[cfg(feature = "native-host")]
pub fn collect_quick_preset_groups(
    catalog: &RuntimeCatalog,
    items: &[Preset],
    is_plugin_command: impl Fn(&str) -> bool,
    app_for_head: impl Fn(&str) -> Option<(String, String)>,
) -> Vec<QuickPresetGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Preset>> = HashMap::new();
    let mut identities: HashMap<String, PresetIdentity> = HashMap::new();

    for preset in items.iter().filter(|p| p.enabled) {
        let cli = SetupTool::detect(catalog, &preset.command);
        let executable = preset.command.split(' ').next().unwrap_or("");
        let trimmed = executable.trim_matches(|c| c == '\'' || c == '"');
        let head = trimmed.rsplit('/').next().unwrap_or("");
        let app = app_for_head(head);

        let id = if let Some(c) = &cli {
            c.id().to_string()
        } else if let Some((app_id, _)) = &app {
            app_id.clone()
        } else {
            format!("custom:{}", preset.id)
        };
        if !groups.contains_key(&id) {
            order.push(id.clone());
            identities.insert(id.clone(), (cli, app));
        }
        groups.entry(id).or_default().push(preset.clone());
        let _ = is_plugin_command(&preset.command);
    }

    let mut result = Vec::new();
    for id in order {
        let Some((cli, app)) = identities.remove(&id) else {
            continue;
        };
        let Some(presets) = groups.remove(&id) else {
            continue;
        };
        if !presets.iter().any(|p| p.quick_launch) {
            continue;
        }
        if let Some(cli) = cli {
            result.push(QuickPresetGroup {
                cli: Some(cli),
                app_id: None,
                app_name: None,
                presets,
            });
        } else if let Some((app_id, app_name)) = app {
            result.push(QuickPresetGroup {
                cli: None,
                app_id: Some(app_id),
                app_name: Some(app_name),
                presets,
            });
        } else {
            result.push(QuickPresetGroup {
                cli: None,
                app_id: None,
                app_name: None,
                presets,
            });
        }
    }
    result
}

/// Splits presets into agents and plugins for the new-session menu.
/// Plugin identity comes from the Host's App catalog.
pub fn split_presets_for_new_session_menu(
    presets: &[Preset],
    is_plugin_command: impl Fn(&str) -> bool,
) -> (Vec<Preset>, Vec<Preset>) {
    let mut agents = Vec::new();
    let mut plugins = Vec::new();
    for preset in presets {
        if is_plugin_command(&preset.command) {
            plugins.push(preset.clone());
        } else {
            agents.push(preset.clone());
        }
    }
    (agents, plugins)
}

/// Entry of the global `presets` array in app-state.json.
#[derive(Debug, Clone)]
pub struct GlobalPresetFile {
    pub id: String,
    pub label: String,
    pub command: String,
    pub project_id: Option<String>,
    pub enabled: Option<bool>,
    pub quick_launch: Option<bool>,
}

impl GlobalPresetFile {
    /// Converts to a Preset, filtering out project-scoped entries.
    ///
    /// Native-host only: needs the generated runtime catalog.
    #[cfg(feature = "native-host")]
    pub fn to_preset(&self, catalog: &RuntimeCatalog) -> Option<Preset> {
        if self.project_id.is_some() {
            return None; // Filter out legacy project presets
        }
        Some(Preset {
            id: self.id.clone(),
            label: self.label.clone(),
            command: self.command.clone(),
            enabled: self.enabled.unwrap_or(true),
            quick_launch: self.quick_launch.unwrap_or(false)
                && SetupTool::detect(catalog, &self.command).is_some(),
        })
    }
}

#[cfg(feature = "native-host")]
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "native-host")]
    fn test_catalog() -> &'static RuntimeCatalog {
        crate::runtime_catalog::builtin_runtime_catalog()
    }

    #[test]
    fn preset_sanitized_clears_quick_launch_on_empty_command() {
        let p = Preset {
            id: "x".into(),
            label: "X".into(),
            command: "   ".into(),
            enabled: true,
            quick_launch: true,
        };
        assert!(!p.sanitized().quick_launch);
        let q = Preset {
            command: "claude".into(),
            ..p.clone()
        };
        // `claude` may or may not be quick-launchable; sanitized only checks non-empty
        assert!(q.sanitized().quick_launch);
    }

    #[test]
    fn new_terminal_pseudo_preset() {
        let p = Preset::new_terminal();
        assert!(p.is_new_terminal());
        assert_eq!(p.id, NEW_TERMINAL_ID);
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn quick_preset_tool_detect_from_command() {
        let catalog = test_catalog();
        // Find a quick-launchable runtime and detect via its first alias.
        let quick: Vec<_> = QuickPresetTool::all_cases(catalog);
        assert!(!quick.is_empty(), "catalog has quick-launch runtimes");
        let tool = &quick[0];
        let meta = tool.metadata(catalog).unwrap();
        let alias = meta.detection.command_aliases.first().unwrap();
        let detected = QuickPresetTool::detect(catalog, alias);
        assert_eq!(detected.as_ref().map(|t| t.id()), Some(tool.id()));
        // Unknown command -> None
        assert!(QuickPresetTool::detect(catalog, "definitely-not-a-tool-xyz").is_none());
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn setup_tool_detect_and_display_name() {
        let catalog = test_catalog();
        let tools = SetupTool::all_cases(catalog);
        assert!(!tools.is_empty());
        let tool = &tools[0];
        assert!(!tool.display_name(catalog).is_empty());
        assert!(SetupTool::detect(catalog, "definitely-not-a-tool-xyz").is_none());
    }

    #[test]
    fn tool_usage_stats_ordering() {
        let a = ToolUsageStats {
            session_count: 10,
            recent_count: 5,
            last_used_ms: Some(1000),
        };
        let b = ToolUsageStats {
            session_count: 100,
            recent_count: 2,
            last_used_ms: Some(2000),
        };
        // Recent activity beats lifetime volume.
        assert!(ToolUsageStats::more_used(&a, &b));
        assert!(!ToolUsageStats::more_used(&b, &a));
    }

    #[test]
    fn tool_usage_summary_formats() {
        let none = ToolUsageStats::NONE;
        assert_eq!(none.summary(1_000_000), None);
        let s = ToolUsageStats {
            session_count: 342,
            recent_count: 1,
            last_used_ms: Some(1_000_000),
        };
        assert_eq!(
            s.summary(1_000_000),
            Some("342 sessions · used today".into())
        );
        assert_eq!(
            s.summary(1_000_000 + 86_400_000),
            Some("342 sessions · used yesterday".into())
        );
        assert_eq!(
            s.summary(1_000_000 + 3 * 86_400_000),
            Some("342 sessions · used 3 days ago".into())
        );
        // > 60 days: no recency suffix
        assert_eq!(
            s.summary(1_000_000 + 61 * 86_400_000),
            Some("342 sessions".into())
        );
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn collect_quick_preset_groups_groups_by_cli() {
        let catalog = test_catalog();
        let quick = QuickPresetTool::all_cases(catalog);
        let tool = &quick[0];
        let alias = tool
            .metadata(catalog)
            .unwrap()
            .detection
            .command_aliases
            .first()
            .unwrap()
            .clone();
        let items = vec![
            Preset {
                id: "p1".into(),
                label: "P1".into(),
                command: alias.clone(),
                enabled: true,
                quick_launch: true,
            },
            Preset {
                id: "p2".into(),
                label: "P2".into(),
                command: format!("{alias} --flag"),
                enabled: true,
                quick_launch: true,
            },
            Preset {
                id: "p3".into(),
                label: "P3".into(),
                command: "plain-shell-cmd".into(),
                enabled: true,
                quick_launch: true,
            },
        ];
        let groups = collect_quick_preset_groups(catalog, &items, |_| false, |_| None);
        // p1+p2 share the CLI group; p3 is custom.
        assert_eq!(groups.len(), 2);
        let cli_group = groups.iter().find(|g| g.cli.is_some()).unwrap();
        assert_eq!(cli_group.presets.len(), 2);
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn global_preset_file_filters_project_scoped() {
        let catalog = test_catalog();
        let file = GlobalPresetFile {
            id: "g1".into(),
            label: "G1".into(),
            command: "somecmd".into(),
            project_id: Some("proj".into()),
            enabled: None,
            quick_launch: None,
        };
        assert!(file.to_preset(catalog).is_none());
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn global_preset_file_converts_global_presets() {
        // Dart: 'converts global presets'.
        let catalog = test_catalog();
        let file = GlobalPresetFile {
            id: "test".into(),
            label: "Test".into(),
            command: "claude".into(),
            project_id: None,
            enabled: None,
            quick_launch: Some(true),
        };
        let preset = file.to_preset(catalog);
        assert!(preset.is_some());
        assert_eq!(preset.unwrap().id, "test");
    }

    #[test]
    fn split_presets_separates_plugins() {
        let items = vec![
            Preset {
                id: "a".into(),
                label: "A".into(),
                command: "claude".into(),
                enabled: true,
                quick_launch: false,
            },
            Preset {
                id: "b".into(),
                label: "B".into(),
                command: "my-plugin-cmd".into(),
                enabled: true,
                quick_launch: false,
            },
        ];
        let (agents, plugins) =
            split_presets_for_new_session_menu(&items, |c| c == "my-plugin-cmd");
        assert_eq!(agents.len(), 1);
        assert_eq!(plugins.len(), 1);
    }
}
