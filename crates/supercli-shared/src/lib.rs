//! Shared pure logic for the Supercli workspace.
//!
//! Tiny leaf crate with **zero non-std dependencies**: both `supercli-core`
//! and `supercli-client` depend on it, so the single implementations of
//! small pure functions stay portable — including the wasm32 Controller
//! core — without dragging either crate's dependency tree into the other.
//!
//! - [`git`] — read a checkout's current git branch without spawning `git`;
//! - [`hash`] — deterministic 64-bit FNV-1a for stable identifiers;
//! - [`validation`] — session ID and artifact path-segment validation.
//!
//! UI display formatting (e.g. find-bar count labels) lives in
//! `supercli-client`, not here: this crate is for std-only primitives.

pub mod git;
pub mod hash;
pub mod validation;
