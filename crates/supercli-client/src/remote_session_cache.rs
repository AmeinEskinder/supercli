//! Per-project cache of archived remote session summaries.
//!
//! Port of Swift `RemoteArchivedSessionSummaryCache` (UnpeelStore.swift).
//! Session ids are Host-global, but tracking ownership by requested project
//! lets a refreshed archive page replace its rows without retaining stale
//! summaries.

use std::collections::{HashMap, HashSet};

/// A session summary carrying its Host-global session id.
pub trait SessionSummaryId {
    fn summary_id(&self) -> &str;
}

/// Per-project cache of archived remote session summaries.
///
/// `replace_project` swaps in a project's fresh summaries, dropping whatever
/// that project previously contributed, without touching other projects'
/// rows. `retain_projects` drops whole projects (and their summaries) that
/// are no longer wanted.
pub struct RemoteArchivedSessionSummaryCache<T> {
    summaries_by_id: HashMap<String, T>,
    session_ids_by_project: HashMap<String, HashSet<String>>,
}

impl<T> RemoteArchivedSessionSummaryCache<T> {
    pub fn new() -> Self {
        Self {
            summaries_by_id: HashMap::new(),
            session_ids_by_project: HashMap::new(),
        }
    }

    /// Look up a cached summary by session id.
    pub fn get(&self, session_id: &str) -> Option<&T> {
        self.summaries_by_id.get(session_id)
    }

    /// All cached session ids across every project.
    pub fn session_ids(&self) -> HashSet<String> {
        self.summaries_by_id.keys().cloned().collect()
    }

    /// Number of cached summaries.
    pub fn len(&self) -> usize {
        self.summaries_by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.summaries_by_id.is_empty()
    }

    /// Drop every cached summary and all project ownership tracking.
    pub fn remove_all(&mut self) {
        self.summaries_by_id.clear();
        self.session_ids_by_project.clear();
    }

    /// Drop the projects (and their summaries) not in `project_ids`.
    pub fn retain_projects(&mut self, project_ids: &HashSet<String>) {
        let removed: Vec<String> = self
            .session_ids_by_project
            .keys()
            .filter(|id| !project_ids.contains(*id))
            .cloned()
            .collect();
        for project_id in removed {
            if let Some(session_ids) = self.session_ids_by_project.remove(&project_id) {
                for session_id in session_ids {
                    self.summaries_by_id.remove(&session_id);
                }
            }
        }
    }
}

impl<T: SessionSummaryId> RemoteArchivedSessionSummaryCache<T> {
    /// Replace one project's summaries: previously cached rows owned by this
    /// project are removed first, then the fresh summaries are stored.
    pub fn replace_project(&mut self, project_id: &str, summaries: Vec<T>) {
        if let Some(old_ids) = self.session_ids_by_project.get(project_id) {
            for session_id in old_ids {
                self.summaries_by_id.remove(session_id);
            }
        }
        let mut session_ids = HashSet::with_capacity(summaries.len());
        for summary in summaries {
            let id = summary.summary_id().to_string();
            session_ids.insert(id.clone());
            self.summaries_by_id.insert(id, summary);
        }
        self.session_ids_by_project
            .insert(project_id.to_string(), session_ids);
    }
}

impl<T> Default for RemoteArchivedSessionSummaryCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Summary {
        id: String,
        title: String,
    }

    impl SessionSummaryId for Summary {
        fn summary_id(&self) -> &str {
            &self.id
        }
    }

    fn summary(id: &str) -> Summary {
        Summary {
            id: id.to_string(),
            title: format!("title-{id}"),
        }
    }

    #[test]
    fn replace_project_stores_summaries_by_id() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1"), summary("s2")]);
        assert_eq!(cache.get("s1"), Some(&summary("s1")));
        assert_eq!(cache.get("s2"), Some(&summary("s2")));
        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache.session_ids(),
            ["s1", "s2"].into_iter().map(String::from).collect()
        );
    }

    #[test]
    fn replace_project_drops_stale_rows_for_that_project() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1"), summary("s2")]);
        cache.replace_project("p1", vec![summary("s2"), summary("s3")]);
        assert!(cache.get("s1").is_none(), "stale s1 must be gone");
        assert!(cache.get("s2").is_some());
        assert!(cache.get("s3").is_some());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn replace_project_refresh_updates_existing_summary() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1")]);
        let updated = Summary {
            id: "s1".to_string(),
            title: "new-title".to_string(),
        };
        cache.replace_project("p1", vec![updated.clone()]);
        assert_eq!(cache.get("s1"), Some(&updated));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn replace_project_leaves_other_projects_alone() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1")]);
        cache.replace_project("p2", vec![summary("s2")]);
        cache.replace_project("p1", vec![summary("s3")]);
        assert!(cache.get("s1").is_none());
        assert!(cache.get("s2").is_some(), "p2 rows must survive p1 refresh");
        assert!(cache.get("s3").is_some());
    }

    #[test]
    fn retain_projects_drops_removed_projects_and_their_summaries() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1")]);
        cache.replace_project("p2", vec![summary("s2")]);
        cache.replace_project("p3", vec![summary("s3")]);
        cache.retain_projects(&["p1", "p3"].into_iter().map(String::from).collect());
        assert!(cache.get("s1").is_some());
        assert!(cache.get("s2").is_none(), "p2 summary must be gone");
        assert!(cache.get("s3").is_some());
    }

    #[test]
    fn retain_projects_with_empty_set_clears_everything() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1")]);
        cache.retain_projects(&HashSet::new());
        assert!(cache.is_empty());
    }

    #[test]
    fn remove_all_clears_summaries_and_ownership() {
        let mut cache = RemoteArchivedSessionSummaryCache::new();
        cache.replace_project("p1", vec![summary("s1")]);
        cache.replace_project("p2", vec![summary("s2")]);
        cache.remove_all();
        assert!(cache.is_empty());
        assert!(cache.session_ids().is_empty());
        // Ownership tracking is gone too: a later replace must not resurrect.
        cache.replace_project("p1", vec![summary("s9")]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get("s1").is_none());
    }

    #[test]
    fn get_missing_id_returns_none() {
        let cache: RemoteArchivedSessionSummaryCache<Summary> =
            RemoteArchivedSessionSummaryCache::new();
        assert!(cache.get("nope").is_none());
    }
}
