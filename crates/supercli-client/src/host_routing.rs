//! Host-scope routing decisions.
//!
//! Ported from the legacy Swift store module (`SupercliStore`) — the "Remote Host
//! scope: display projection and verb plumbing" extension. These are the pure
//! predicates the store uses to decide, per scope, whether views render the
//! Host projection and whether verbs route through the Host.

use crate::scope::SelectedHostScope;

/// Whether views should render the Host projection for `scope`.
///
/// Local scope shows the projection once the local Host client has started
/// AND its projection is ready; non-local scopes always show it (their only
/// truth is the projection).
pub fn should_display_host_projection(
    scope: &SelectedHostScope,
    local_client_started: bool,
    local_projection_ready: bool,
) -> bool {
    !matches!(scope, SelectedHostScope::Local) || (local_client_started && local_projection_ready)
}

/// Whether a verb for `scope` routes through the Host.
///
/// Once the Local Host client starts, semantic effects fail closed to its
/// workspace worker — a missing socket must never silently reactivate a
/// duplicate engine. Non-local scopes route through the Host whenever the
/// projected entity exists.
pub fn should_route_host_verb(
    scope: &SelectedHostScope,
    local_client_started: bool,
    projected_entity_exists: bool,
) -> bool {
    match scope {
        SelectedHostScope::Local => local_client_started,
        SelectedHostScope::LocalWorkspace { .. } | SelectedHostScope::Remote { .. } => {
            projected_entity_exists
        }
    }
}

/// Whether the current scope uses Host control at all: any non-local scope,
/// or Local once its Host client has started.
pub fn scope_uses_host_control(scope: &SelectedHostScope, local_client_started: bool) -> bool {
    !matches!(scope, SelectedHostScope::Local) || local_client_started
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> SelectedHostScope {
        SelectedHostScope::Local
    }

    fn remote() -> SelectedHostScope {
        SelectedHostScope::Remote {
            host_id: "h1".to_string(),
        }
    }

    fn workspace() -> SelectedHostScope {
        SelectedHostScope::LocalWorkspace {
            home: "/tmp/ws".to_string(),
            name: "ws".to_string(),
        }
    }

    #[test]
    fn local_projection_needs_client_and_ready() {
        assert!(!should_display_host_projection(&local(), false, false));
        assert!(!should_display_host_projection(&local(), true, false));
        assert!(!should_display_host_projection(&local(), false, true));
        assert!(should_display_host_projection(&local(), true, true));
    }

    #[test]
    fn nonlocal_projection_always_shown() {
        assert!(should_display_host_projection(&remote(), false, false));
        assert!(should_display_host_projection(&workspace(), false, false));
    }

    #[test]
    fn local_verb_routes_only_when_client_started() {
        assert!(!should_route_host_verb(&local(), false, true));
        assert!(should_route_host_verb(&local(), true, false));
        assert!(should_route_host_verb(&local(), true, true));
    }

    #[test]
    fn nonlocal_verb_routes_when_projected_entity_exists() {
        assert!(!should_route_host_verb(&remote(), true, false));
        assert!(should_route_host_verb(&remote(), false, true));
        assert!(!should_route_host_verb(&workspace(), false, false));
        assert!(should_route_host_verb(&workspace(), true, true));
    }

    #[test]
    fn scope_uses_host_control_cases() {
        assert!(!scope_uses_host_control(&local(), false));
        assert!(scope_uses_host_control(&local(), true));
        assert!(scope_uses_host_control(&remote(), false));
        assert!(scope_uses_host_control(&workspace(), false));
    }
}
