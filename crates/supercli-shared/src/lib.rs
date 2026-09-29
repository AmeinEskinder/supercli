//! Shared pure logic for the Supercli workspace.
//!
//! Tiny leaf crate with **zero non-std dependencies**: both `supercli-core`
//! and `supercli-client` depend on it, so the single implementations of
//! small pure functions stay portable — including the wasm32 Controller
//! core — without dragging either crate's dependency tree into the other.
//!
//! - [`git`] — read a checkout's current git branch without spawning `git`;
//! - [`hash`] — deterministic 64-bit FNV-1a for stable identifiers.

pub mod git;
pub mod hash;
