//! Project tree + session filing rules, ported from
//! `clients/native/UnpeelNative/Sources/UnpeelNative/SessionMoveRules.swift`
//! and the project-grouping half of
//! `clients/native/UnpeelNative/Sources/UnpeelNative/Views/ProjectSidebarView.swift`.
//!
//! Filing is display-only (the shared `project-override.json` marker): a
//! session's shell runs in exactly one checkout, so the only legal filing
//! targets are the session's home project and plain organizational groups
//! directly under it. The Host enforces the same rule in
//! `controller_host::validate_session_project_target` for every Controller.

use serde::{Deserialize, Serialize};

/// A project node in the sidebar tree. Mirrors the Swift `Project` fields
/// the move rules read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_project_id: Option<String>,
    #[serde(default)]
    pub is_worktree: bool,
    /// Whether the row accepts a session drop (plain organizational group).
    #[serde(default)]
    pub accepts_session_drop: bool,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub session_ids: Vec<String>,
}

/// A session's checkout-bound project: walk up through plain groups until a
/// worktree project or a root is reached.
pub fn home_project_id(
    project_id: &str,
    projects_by_id: &std::collections::HashMap<String, Project>,
) -> String {
    let mut id = project_id.to_string();
    let mut hops = 0;
    while let Some(project) = projects_by_id.get(&id) {
        if project.is_worktree {
            break;
        }
        match &project.parent_project_id {
            Some(parent) if hops < 16 => {
                id = parent.clone();
                hops += 1;
            }
            _ => break,
        }
    }
    id
}

/// True when the session's home is a git worktree: its row may only be
/// filed inside that worktree.
pub fn is_worktree_bound(
    session_project_id: &str,
    projects_by_id: &std::collections::HashMap<String, Project>,
) -> bool {
    let home = home_project_id(session_project_id, projects_by_id);
    projects_by_id.get(&home).is_some_and(|p| p.is_worktree)
}

/// Whether `target_id` is a legal filing destination for a session whose
/// manifest names `session_project_id` and which currently renders under
/// `effective_project_id` (its override, or the manifest project).
pub fn can_file(
    session_project_id: &str,
    effective_project_id: &str,
    target_id: &str,
    projects_by_id: &std::collections::HashMap<String, Project>,
) -> bool {
    let Some(target) = projects_by_id.get(target_id) else {
        return false;
    };
    let home = home_project_id(session_project_id, projects_by_id);
    let target_is_home = target_id == home;
    let target_is_plain_group =
        target.accepts_session_drop && target.parent_project_id.as_deref() == Some(home.as_str());
    if !(target_is_home || target_is_plain_group) {
        return false;
    }
    effective_project_id != target_id
}

/// "Move to ▸" destinations: the home project plus its plain groups, in
/// sidebar order, minus the current location and any hidden group.
pub fn destinations(
    session_project_id: &str,
    effective_project_id: &str,
    projects_by_id: &std::collections::HashMap<String, Project>,
    is_hidden_group: &dyn Fn(&str) -> bool,
) -> Vec<Project> {
    let home_id = home_project_id(session_project_id, projects_by_id);
    let Some(home) = projects_by_id.get(&home_id) else {
        return Vec::new();
    };
    let mut groups: Vec<&Project> = projects_by_id
        .values()
        .filter(|p| {
            p.parent_project_id.as_deref() == Some(home_id.as_str())
                && p.accepts_session_drop
                && !is_hidden_group(&p.id)
        })
        .collect();
    groups.sort_by_key(|p| p.sort_order.unwrap_or(0));
    std::iter::once(home)
        .chain(groups)
        .filter(|p| p.id != effective_project_id)
        .cloned()
        .collect()
}

/// A drag hovering a row owned by `hovered_project_id` crosses a checkout
/// boundary when the two homes differ and at least one of them is a
/// worktree. Such a release is refused with the "no" shake instead of
/// silently landing nowhere.
pub fn crosses_checkout(
    session_project_id: &str,
    hovered_project_id: &str,
    projects_by_id: &std::collections::HashMap<String, Project>,
) -> bool {
    let session_home = home_project_id(session_project_id, projects_by_id);
    let hovered_home = home_project_id(hovered_project_id, projects_by_id);
    if session_home == hovered_home {
        return false;
    }
    projects_by_id
        .get(&session_home)
        .is_some_and(|p| p.is_worktree)
        || projects_by_id
            .get(&hovered_home)
            .is_some_and(|p| p.is_worktree)
}

/// Group projects into a sidebar tree: roots first (sorted by sort_order),
/// each with its direct children. Mirrors the project-grouping rendering.
pub fn project_tree(projects: &[Project]) -> Vec<(Project, Vec<Project>)> {
    let mut roots: Vec<&Project> = projects
        .iter()
        .filter(|p| p.parent_project_id.is_none())
        .collect();
    roots.sort_by_key(|p| p.sort_order.unwrap_or(0));
    roots
        .into_iter()
        .map(|root| {
            let mut children: Vec<Project> = projects
                .iter()
                .filter(|p| p.parent_project_id.as_deref() == Some(root.id.as_str()))
                .cloned()
                .collect();
            children.sort_by_key(|p| p.sort_order.unwrap_or(0));
            (root.clone(), children)
        })
        .collect()
}

/// Dioxus component: the grouped project sidebar.
pub mod component {
    use super::*;
    use dioxus::prelude::*;

    #[component]
    pub fn ProjectTreeView(
        projects: Vec<Project>,
        selected_session_id: Option<String>,
        on_select_session: EventHandler<String>,
    ) -> Element {
        let tree = project_tree(&projects);
        rsx! {
            ul { class: "project-tree",
                for (root, children) in tree {
                    li {
                        key: "{root.id}",
                        class: "project-group",
                        div { class: "project-group-name", "{root.name}" }
                        ul { class: "project-sessions",
                            for sid in root.session_ids.iter().chain(children.iter().flat_map(|c| c.session_ids.iter())) {
                                li {
                                    key: "{sid}",
                                    button {
                                        class: if Some(sid.as_str()) == selected_session_id.as_deref() { "session-row selected" } else { "session-row" },
                                        onclick: {
                                            let sid = sid.clone();
                                            move |_| on_select_session.call(sid.clone())
                                        },
                                        "{sid}"
                                    }
                                }
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
    use std::collections::HashMap;

    fn projects() -> HashMap<String, Project> {
        let mk = |id: &str,
                  name: &str,
                  parent: Option<&str>,
                  worktree: bool,
                  accepts: bool,
                  sort: Option<i64>| {
            (
                id.to_string(),
                Project {
                    id: id.to_string(),
                    name: name.to_string(),
                    parent_project_id: parent.map(str::to_string),
                    is_worktree: worktree,
                    accepts_session_drop: accepts,
                    sort_order: sort,
                    session_ids: Vec::new(),
                },
            )
        };
        HashMap::from([
            mk("root", "Root", None, false, false, Some(0)),
            mk("g1", "Group 1", Some("root"), false, true, Some(1)),
            mk("g2", "Group 2", Some("root"), false, true, Some(2)),
            mk("hidden", "Hidden", Some("root"), false, true, Some(3)),
            mk("wt", "Worktree", Some("root"), true, false, Some(4)),
            mk("other", "Other", None, false, false, Some(5)),
        ])
    }

    #[test]
    fn home_walks_up_through_plain_groups() {
        let p = projects();
        assert_eq!(home_project_id("g1", &p), "root");
        assert_eq!(home_project_id("root", &p), "root");
        assert_eq!(home_project_id("wt", &p), "wt");
        assert_eq!(home_project_id("missing", &p), "missing");
    }

    #[test]
    fn worktree_bound_sessions_stay_put() {
        let p = projects();
        assert!(is_worktree_bound("wt", &p));
        assert!(!is_worktree_bound("g1", &p));
        // A worktree session cannot be filed into the root or a plain group.
        assert!(!can_file("wt", "wt", "root", &p));
        assert!(!can_file("wt", "wt", "g1", &p));
        assert!(!can_file("wt", "wt", "wt", &p)); // same location
    }

    #[test]
    fn can_file_home_and_plain_groups_only() {
        let p = projects();
        // Session in g1: home is root.
        assert!(can_file("g1", "g1", "root", &p));
        assert!(can_file("g1", "g1", "g2", &p));
        // Not the current location.
        assert!(!can_file("g1", "root", "root", &p));
        // Not a worktree, not a plain group, not another root.
        assert!(!can_file("g1", "g1", "wt", &p));
        assert!(!can_file("g1", "g1", "other", &p));
        assert!(!can_file("g1", "g1", "missing", &p));
    }

    #[test]
    fn destinations_exclude_current_and_hidden() {
        let p = projects();
        let dests = destinations("g1", "g1", &p, &|id| id == "hidden");
        let ids: Vec<&str> = dests.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["root", "g2"]); // current location g1 and hidden excluded
                                             // Sidebar order respected.
        let dests2 = destinations("g1", "root", &p, &|_| false);
        let ids2: Vec<&str> = dests2.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids2, vec!["g1", "g2", "hidden"]);
    }

    #[test]
    fn crosses_checkout_detection() {
        let p = projects();
        // g1 (home root) vs wt (home wt): one side is a worktree → crosses.
        assert!(crosses_checkout("g1", "wt", &p));
        assert!(crosses_checkout("wt", "g1", &p));
        // Same home → no crossing.
        assert!(!crosses_checkout("g1", "g2", &p));
        // Different roots, neither a worktree → no crossing.
        assert!(!crosses_checkout("g1", "other", &p));
    }

    #[test]
    fn tree_groups_children_under_roots() {
        let p = projects();
        let all: Vec<Project> = p.values().cloned().collect();
        let tree = project_tree(&all);
        assert_eq!(tree.len(), 2); // root, other
        assert_eq!(tree[0].0.id, "root");
        let child_ids: Vec<&str> = tree[0].1.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(child_ids, vec!["g1", "g2", "hidden", "wt"]);
    }
}
