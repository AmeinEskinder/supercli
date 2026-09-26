//! The Supercli Apps API — what a terminal app needs to be a good citizen of
//! an Supercli session, beyond drawing itself.
//!
//! The first capability is agent-pane integration ([`agent`]): detect an
//! agent session in the app's own sidebar group and paste a reference into
//! its input, with an honest-label prober and a clipboard fallback. All of
//! it rides `supercli-host __mcp__` — the same unified MCP server agents use
//! — so group membership and write policy are enforced by Supercli itself,
//! never reimplemented per app.

pub mod agent;
