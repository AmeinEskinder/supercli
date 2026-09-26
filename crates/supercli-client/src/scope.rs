//! Controller-side host scope: the local-execution safety boundary.
//!
//! Ported from `clients/legacy/native/SupercliNative/Sources/SupercliNative/SelectedHostScope.swift`.
//! This is deliberately smaller than a session backend: it exists so every
//! local spawn is guarded before remote scope becomes a product surface.

/// The local-execution safety boundary for the Host picker.
///
/// - `Local`: this instance's own home. Local execution permitted.
/// - `Remote`: a paired/SSH host. Pure client; no filesystem verbs.
/// - `LocalWorkspace`: another LOCAL workspace scoped through the loopback
///   gateway. It is NOT this instance's home: local execution stays refused
///   exactly as in remote scope, and every verb rides the selected Host
///   connection. Keyed by the normalized workspace home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedHostScope {
    Local,
    Remote { host_id: String },
    LocalWorkspace { home: String, name: String },
}

impl SelectedHostScope {
    /// True only for this instance's own local scope.
    pub fn permits_local_execution(&self) -> bool {
        *self == SelectedHostScope::Local
    }

    /// True whenever the selected scope is a workspace on THIS machine —
    /// this instance's own Local scope, or another local workspace reached
    /// over the loopback gateway. Filesystem/state verbs are valid here;
    /// a true `.remote` scope is NOT a local machine.
    ///
    /// Session HOSTING still keys off `permits_local_execution` alone.
    pub fn is_local_machine(&self) -> bool {
        match self {
            SelectedHostScope::Local | SelectedHostScope::LocalWorkspace { .. } => true,
            SelectedHostScope::Remote { .. } => false,
        }
    }

    /// Multiple panes are a Controller rendering capability, not a Host or
    /// workspace capability. Every scope uses the same pane model.
    pub fn supports_session_panes(&self) -> bool {
        true
    }

    /// Stable key for this Controller window's presentation state. Names are
    /// deliberately excluded: renaming a workspace must not orphan its pane
    /// layout.
    pub fn pane_scope_id(&self) -> String {
        match self {
            SelectedHostScope::Local => "local".to_string(),
            SelectedHostScope::LocalWorkspace { home, .. } => format!("workspace:{home}"),
            SelectedHostScope::Remote { host_id } => format!("host:{host_id}"),
        }
    }

    /// The `SUPERCLI_HOME` that a local-against-home filesystem/state verb
    /// must target: `None` for `Local` (this instance's own home) and for
    /// `Remote` (those verbs never run there).
    pub fn scoped_local_home(&self) -> Option<&str> {
        match self {
            SelectedHostScope::LocalWorkspace { home, .. } => Some(home),
            _ => None,
        }
    }

    pub fn remote_host_id(&self) -> Option<&str> {
        match self {
            SelectedHostScope::Remote { host_id } => Some(host_id),
            _ => None,
        }
    }

    pub fn local_workspace_home(&self) -> Option<&str> {
        match self {
            SelectedHostScope::LocalWorkspace { home, .. } => Some(home),
            _ => None,
        }
    }

    pub fn local_workspace_name(&self) -> Option<&str> {
        match self {
            SelectedHostScope::LocalWorkspace { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Additive field in the session-host launch JSON. Rust defaults a
    /// missing value to local for compatibility.
    pub fn session_launch_wire_value(&self) -> &'static str {
        match self {
            SelectedHostScope::Local => "local",
            SelectedHostScope::Remote { .. } | SelectedHostScope::LocalWorkspace { .. } => {
                "remote_controller"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from SelectedHostScopeTests.swift.

    #[test]
    fn session_panes_are_a_controller_capability_in_every_scope() {
        let secondary_workspace = SelectedHostScope::LocalWorkspace {
            home: "/Users/me/.supercli/profiles/writing".to_string(),
            name: "Writing".to_string(),
        };

        assert!(SelectedHostScope::Local.supports_session_panes());
        assert!(secondary_workspace.supports_session_panes());
        assert!(!secondary_workspace.permits_local_execution());
        assert!(SelectedHostScope::Remote {
            host_id: "studio-mac".to_string()
        }
        .supports_session_panes());
        assert_eq!(SelectedHostScope::Local.pane_scope_id(), "local");
        assert_eq!(
            secondary_workspace.pane_scope_id(),
            SelectedHostScope::LocalWorkspace {
                home: "/Users/me/.supercli/profiles/writing".to_string(),
                name: "Renamed Writing".to_string(),
            }
            .pane_scope_id()
        );
        assert_ne!(
            secondary_workspace.pane_scope_id(),
            SelectedHostScope::Remote {
                host_id: "studio-mac".to_string()
            }
            .pane_scope_id()
        );
    }

    #[test]
    fn remote_scope_is_never_this_instances_home() {
        let scope = SelectedHostScope::Remote {
            host_id: "studio-mac".to_string(),
        };

        assert!(!scope.permits_local_execution());
        assert!(SelectedHostScope::Local.permits_local_execution());
        assert_eq!(scope.session_launch_wire_value(), "remote_controller");
        assert_eq!(scope.remote_host_id(), Some("studio-mac"));
        assert_eq!(SelectedHostScope::Local.remote_host_id(), None);
    }

    #[test]
    fn local_workspace_scope_is_remote_like_at_every_choke_point() {
        let scope = SelectedHostScope::LocalWorkspace {
            home: "/Users/me/.supercli/profiles/writing".to_string(),
            name: "Writing".to_string(),
        };

        assert!(!scope.permits_local_execution());
        assert_eq!(scope.session_launch_wire_value(), "remote_controller");
        assert_eq!(scope.remote_host_id(), None);
        assert_eq!(
            scope.local_workspace_home(),
            Some("/Users/me/.supercli/profiles/writing")
        );
        assert_eq!(scope.local_workspace_name(), Some("Writing"));
        assert_eq!(SelectedHostScope::Local.local_workspace_home(), None);
        assert_eq!(
            SelectedHostScope::Remote {
                host_id: "studio-mac".to_string()
            }
            .local_workspace_home(),
            None
        );
    }

    #[test]
    fn is_local_machine_distinguishes_remote() {
        assert!(SelectedHostScope::Local.is_local_machine());
        assert!(SelectedHostScope::LocalWorkspace {
            home: "/h".to_string(),
            name: "N".to_string(),
        }
        .is_local_machine());
        assert!(!SelectedHostScope::Remote {
            host_id: "x".to_string()
        }
        .is_local_machine());
    }

    #[test]
    fn scoped_local_home_only_for_workspace() {
        assert_eq!(SelectedHostScope::Local.scoped_local_home(), None);
        assert_eq!(
            SelectedHostScope::Remote {
                host_id: "x".to_string()
            }
            .scoped_local_home(),
            None
        );
        assert_eq!(
            SelectedHostScope::LocalWorkspace {
                home: "/h".to_string(),
                name: "N".to_string(),
            }
            .scoped_local_home(),
            Some("/h")
        );
    }
}
