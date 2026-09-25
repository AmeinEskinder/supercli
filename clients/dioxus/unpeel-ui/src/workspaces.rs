//! Workspace open targets, workspace/worktree models, and the workspaces +
//! worktrees settings data. Ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/WorkspaceOpenTarget.swift`,
//! `Views/WorkspacesSettingsPanel.swift`, `Views/WorktreesSettingsPanel.swift`,
//! `Views/RemoteFolderPicker.swift`, `Views/SidebarWorkspaceDots.swift`, and
//! `Views/SidebarWorkspaceSelector.swift`.
//!
//! The registry/pool/git operations themselves are Host-side (no Controller
//! verbs exist for them) and stay `blocked` in the audit; this module ports
//! everything the Controller owns: the open-target catalog, the worktree
//! model, and the settings surfaces.

use crate::i18n::t;
use serde::{Deserialize, Serialize};

/// Where a workspace folder can be opened. Mirrors `WorkspaceOpenTarget`
/// (titles kept verbatim).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceOpenTarget {
    Vscode,
    Cursor,
    Zed,
    Idea,
    Webstorm,
    GithubDesktop,
    Fork,
    Tower,
    Sourcetree,
    Gitkraken,
    SublimeMerge,
    Finder,
    Terminal,
    Iterm2,
    Ghostty,
    Warp,
    Wezterm,
    Kitty,
    Alacritty,
    Tabby,
    Hyper,
    Rio,
    Wave,
    Xcode,
}

impl WorkspaceOpenTarget {
    pub fn all() -> [WorkspaceOpenTarget; 24] {
        [
            WorkspaceOpenTarget::Vscode,
            WorkspaceOpenTarget::Cursor,
            WorkspaceOpenTarget::Zed,
            WorkspaceOpenTarget::Idea,
            WorkspaceOpenTarget::Webstorm,
            WorkspaceOpenTarget::GithubDesktop,
            WorkspaceOpenTarget::Fork,
            WorkspaceOpenTarget::Tower,
            WorkspaceOpenTarget::Sourcetree,
            WorkspaceOpenTarget::Gitkraken,
            WorkspaceOpenTarget::SublimeMerge,
            WorkspaceOpenTarget::Finder,
            WorkspaceOpenTarget::Terminal,
            WorkspaceOpenTarget::Iterm2,
            WorkspaceOpenTarget::Ghostty,
            WorkspaceOpenTarget::Warp,
            WorkspaceOpenTarget::Wezterm,
            WorkspaceOpenTarget::Kitty,
            WorkspaceOpenTarget::Alacritty,
            WorkspaceOpenTarget::Tabby,
            WorkspaceOpenTarget::Hyper,
            WorkspaceOpenTarget::Rio,
            WorkspaceOpenTarget::Wave,
            WorkspaceOpenTarget::Xcode,
        ]
    }

    pub fn title(&self) -> String {
        match self {
            WorkspaceOpenTarget::Vscode => t("workspaces.vs_code"),
            WorkspaceOpenTarget::Cursor => t("workspaces.cursor"),
            WorkspaceOpenTarget::Zed => t("workspaces.zed"),
            WorkspaceOpenTarget::Idea => t("workspaces.intellij"),
            WorkspaceOpenTarget::Webstorm => t("workspaces.webstorm"),
            WorkspaceOpenTarget::GithubDesktop => t("workspaces.github_desktop"),
            WorkspaceOpenTarget::Fork => t("workspaces.fork"),
            WorkspaceOpenTarget::Tower => t("workspaces.tower"),
            WorkspaceOpenTarget::Sourcetree => t("workspaces.sourcetree"),
            WorkspaceOpenTarget::Gitkraken => t("workspaces.gitkraken"),
            WorkspaceOpenTarget::SublimeMerge => t("workspaces.sublime_merge"),
            WorkspaceOpenTarget::Finder => t("workspaces.finder"),
            WorkspaceOpenTarget::Terminal => t("workspaces.terminal"),
            WorkspaceOpenTarget::Iterm2 => t("workspaces.iterm2"),
            WorkspaceOpenTarget::Ghostty => t("workspaces.ghostty"),
            WorkspaceOpenTarget::Warp => t("workspaces.warp"),
            WorkspaceOpenTarget::Wezterm => t("workspaces.wezterm"),
            WorkspaceOpenTarget::Kitty => t("workspaces.kitty"),
            WorkspaceOpenTarget::Alacritty => t("workspaces.alacritty"),
            WorkspaceOpenTarget::Tabby => t("workspaces.tabby"),
            WorkspaceOpenTarget::Hyper => t("workspaces.hyper"),
            WorkspaceOpenTarget::Rio => t("workspaces.rio"),
            WorkspaceOpenTarget::Wave => t("workspaces.wave"),
            WorkspaceOpenTarget::Xcode => t("workspaces.xcode"),
        }
    }

    /// Binary names to probe on `PATH` (portable replacement for the macOS
    /// bundle-identifier lookup).
    pub fn binary_names(&self) -> &'static [&'static str] {
        match self {
            WorkspaceOpenTarget::Vscode => &["code", "code-insiders"],
            WorkspaceOpenTarget::Cursor => &["cursor"],
            WorkspaceOpenTarget::Zed => &["zed"],
            WorkspaceOpenTarget::Idea => &["idea", "intellij-idea-ultimate-edition"],
            WorkspaceOpenTarget::Webstorm => &["webstorm"],
            WorkspaceOpenTarget::GithubDesktop => &["github-desktop"],
            WorkspaceOpenTarget::Fork => &["fork"],
            WorkspaceOpenTarget::Tower => &["tower", "gittower"],
            WorkspaceOpenTarget::Sourcetree => &["sourcetree"],
            WorkspaceOpenTarget::Gitkraken => &["gitkraken"],
            WorkspaceOpenTarget::SublimeMerge => &["smerge"],
            WorkspaceOpenTarget::Finder => &[],
            WorkspaceOpenTarget::Terminal => &[],
            WorkspaceOpenTarget::Iterm2 => &[],
            WorkspaceOpenTarget::Ghostty => &["ghostty"],
            WorkspaceOpenTarget::Warp => &["warp-terminal"],
            WorkspaceOpenTarget::Wezterm => &["wezterm"],
            WorkspaceOpenTarget::Kitty => &["kitty"],
            WorkspaceOpenTarget::Alacritty => &["alacritty"],
            WorkspaceOpenTarget::Tabby => &["tabby"],
            WorkspaceOpenTarget::Hyper => &["hyper"],
            WorkspaceOpenTarget::Rio => &["rio"],
            WorkspaceOpenTarget::Wave => &["wave"],
            WorkspaceOpenTarget::Xcode => &[],
        }
    }

    /// Whether this target can open a folder on this OS at all (Finder /
    /// Terminal / Xcode are macOS-only; the rest probe `PATH`).
    pub fn is_available_on_this_platform(&self) -> bool {
        !matches!(
            self,
            WorkspaceOpenTarget::Finder
                | WorkspaceOpenTarget::Terminal
                | WorkspaceOpenTarget::Iterm2
                | WorkspaceOpenTarget::Xcode
        ) || cfg!(target_os = "macos")
    }
}

/// Open `path` in `target`. Portable replacement for the AppKit
/// `NSWorkspace.open` path: on macOS uses `open -a`, elsewhere probes the
/// target's binaries on `PATH` and falls back to `xdg-open`.
pub fn open_in_target(target: WorkspaceOpenTarget, path: &str) -> Result<(), String> {
    if !target.is_available_on_this_platform() {
        return Err(format!(
            "{} is not available on this platform",
            target.title()
        ));
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg(target.title())
            .arg(path)
            .status()
            .map_err(|e| format!("could not launch {}: {e}", target.title()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{} exited with {status}", target.title()))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        for binary in target.binary_names() {
            if path_on_path(binary).is_some() {
                let status = std::process::Command::new(binary)
                    .arg(path)
                    .status()
                    .map_err(|e| format!("could not launch {binary}: {e}"))?;
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("{binary} exited with {status}"))
                };
            }
        }
        // Last resort: the desktop's default handler.
        let status = std::process::Command::new("xdg-open")
            .arg(path)
            .status()
            .map_err(|e| format!("no handler for {}: {e}", target.title()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("xdg-open exited with {status}"))
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn path_on_path(binary: &str) -> Option<std::path::PathBuf> {
    std::env::var_os({ t("workspaces.path") }).and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(binary))
            .find(|p| p.is_file())
    })
}

/// A local workspace: a fully isolated Unpeel home (own sessions, projects,
/// settings, phone-pairing identity). Mirrors the workspaces settings model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRef {
    pub id: String,
    pub name: String,
    pub home_path: String,
    pub is_default: bool,
}

/// A git worktree derived from a project. Mirrors the worktrees panel model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
    /// Sessions currently rooted in this worktree.
    pub session_count: usize,
}

impl WorktreeInfo {
    pub fn display_name(&self) -> String {
        if self.branch.is_empty() {
            self.path.clone()
        } else {
            format!("{} ({})", self.branch, self.path)
        }
    }
}

/// Worktrees settings. Mirrors `WorktreesSettingsPanel`'s persisted knobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorktreeSettings {
    /// Prune worktrees whose sessions all ended more than this many days ago.
    /// 0 disables auto-prune.
    pub auto_prune_after_days: u32,
    /// Refuse to create a worktree when the project has uncommitted changes.
    pub require_clean_tree: bool,
}

impl Default for WorktreeSettings {
    fn default() -> Self {
        Self {
            auto_prune_after_days: 0,
            require_clean_tree: true,
        }
    }
}

/// Whether a worktree may be created right now under the current settings.
pub fn can_create_worktree(settings: &WorktreeSettings, tree_is_dirty: bool) -> bool {
    !(settings.require_clean_tree && tree_is_dirty)
}

/// Remote folder picker state. Mirrors `RemoteFolderPicker`'s validation:
/// the picked path must be absolute and non-empty; the Host resolves the
/// rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteFolderPick {
    pub path: String,
}

impl RemoteFolderPick {
    pub fn is_valid(&self) -> bool {
        let trimmed = self.path.trim();
        !trimmed.is_empty() && trimmed.starts_with('/')
    }
}

/// The flat workspace picker rows. Mirrors `SidebarWorkspaceSelector`.
pub fn workspace_picker_rows(
    workspaces: &[WorkspaceRef],
    selected_id: &str,
) -> Vec<(String, bool)> {
    workspaces
        .iter()
        .map(|w| (w.name.clone(), w.id == selected_id))
        .collect()
}

/// Dioxus components: workspace open menu + worktrees settings panel.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn WorkspaceOpenMenu(
        targets: Vec<WorkspaceOpenTarget>,
        on_open: EventHandler<WorkspaceOpenTarget>,
    ) -> Element {
        rsx! {
            ul { class: "workspace-open-menu",
                for t in targets {
                    li {
                        key: "{t.title()}",
                        button {
                            class: "open-target-row",
                            disabled: !t.is_available_on_this_platform(),
                            onclick: move |_| on_open.call(t),
                            "{t.title()}"
                        }
                    }
                }
            }
        }
    }

    #[component]
    pub fn WorktreesSettingsPanel(
        settings: WorktreeSettings,
        worktrees: Vec<WorktreeInfo>,
        on_settings: EventHandler<WorktreeSettings>,
        on_prune: EventHandler<String>,
    ) -> Element {
        rsx! {
            div { class: "worktrees-settings",
                h2 { {t("workspaces.git_worktrees")} }
                label { class: "settings-row",
                    {t("workspaces.require_a_clean_tree_before_creating_a_w")}
                    input {
                        r#type: "checkbox",
                        checked: settings.require_clean_tree,
                        onchange: {
                            let settings = settings.clone();
                            move |e: Event<FormData>| {
                                let mut s = settings.clone();
                                s.require_clean_tree = e.value() == "true";
                                on_settings.call(s);
                            }
                        }
                    }
                }
                ul { class: "worktree-list",
                    for w in worktrees {
                        li {
                            key: "{w.path}",
                            span { class: "worktree-name", "{w.display_name()}" }
                            if w.is_dirty { span { class: "worktree-dirty", {t("workspaces.uncommitted_changes")} } }
                            span { class: "worktree-sessions", "{w.session_count} sessions" }
                            button {
                                class: "worktree-prune",
                                onclick: {
                                    let path = w.path.clone();
                                    move |_| on_prune.call(path.clone())
                                },
                                {t("workspaces.remove")}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_target_catalog_complete() {
        let all = WorkspaceOpenTarget::all();
        assert_eq!(all.len(), 24);
        assert_eq!(WorkspaceOpenTarget::Vscode.title(), "VS Code");
        assert_eq!(WorkspaceOpenTarget::Iterm2.title(), "iTerm2");
        assert_eq!(WorkspaceOpenTarget::SublimeMerge.title(), "Sublime Merge");
        // Titles are unique.
        let mut titles: Vec<&str> = all.iter().map(|t| t.title()).collect();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), 24);
    }

    #[test]
    fn platform_gating() {
        // Finder/Terminal/Xcode are macOS-only.
        assert_eq!(
            WorkspaceOpenTarget::Finder.is_available_on_this_platform(),
            cfg!(target_os = "macos")
        );
        assert!(WorkspaceOpenTarget::Vscode.is_available_on_this_platform());
    }

    #[test]
    fn open_in_target_rejects_unavailable() {
        // This VM is Linux, so Finder must be rejected without spawning anything.
        if !cfg!(target_os = "macos") {
            let err = open_in_target(WorkspaceOpenTarget::Finder, "/tmp").unwrap_err();
            assert!(err.contains("not available on this platform"));
        }
    }

    #[test]
    fn worktree_settings_defaults_and_gate() {
        let s = WorktreeSettings::default();
        assert!(s.require_clean_tree);
        assert_eq!(s.auto_prune_after_days, 0);
        assert!(can_create_worktree(&s, false));
        assert!(!can_create_worktree(&s, true));
        let lax = WorktreeSettings {
            require_clean_tree: false,
            auto_prune_after_days: 7,
        };
        assert!(can_create_worktree(&lax, true));
    }

    #[test]
    fn worktree_display_name() {
        let w = WorktreeInfo {
            path: "/repo/wt-1".into(),
            branch: "feat".into(),
            is_dirty: false,
            session_count: 2,
        };
        assert_eq!(w.display_name(), "feat (/repo/wt-1)");
        let bare = WorktreeInfo {
            path: "/repo/wt-2".into(),
            branch: String::new(),
            is_dirty: true,
            session_count: 0,
        };
        assert_eq!(bare.display_name(), "/repo/wt-2");
    }

    #[test]
    fn remote_folder_pick_validation() {
        assert!(RemoteFolderPick {
            path: "/home/u/proj".into()
        }
        .is_valid());
        assert!(!RemoteFolderPick { path: "  ".into() }.is_valid());
        assert!(!RemoteFolderPick {
            path: "relative/path".into()
        }
        .is_valid());
        assert!(!RemoteFolderPick::default().is_valid());
    }

    #[test]
    fn workspace_picker_marks_selected() {
        let ws = vec![
            WorkspaceRef {
                id: "a".into(),
                name: "Main".into(),
                home_path: "/h/a".into(),
                is_default: true,
            },
            WorkspaceRef {
                id: "b".into(),
                name: "Side".into(),
                home_path: "/h/b".into(),
                is_default: false,
            },
        ];
        let rows = workspace_picker_rows(&ws, "b");
        assert_eq!(
            rows,
            vec![("Main".to_string(), false), ("Side".to_string(), true)]
        );
    }
}
