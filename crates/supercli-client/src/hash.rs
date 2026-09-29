//! Deterministic hashing primitives shared by the Host and portable clients.
//!
//! [`fnv1a`] is the 64-bit FNV-1a hash used for stable identifiers (worktree
//! directory slugs, dev UserDefaults suite names). Swift's `String.hashValue`
//! is salted per process, so a deterministic hash is required wherever the
//! value must survive a relaunch.
//!
//! Single implementation: previously duplicated as `supercli-core`'s private
//! `worktrees::fnv1a` and `launch_config::stable_hash` (both now call this).
//! This crate is the home because the workspace dependency graph forbids
//! `supercli-client` from depending on `supercli-core`
//! (`core → connector → client` would cycle); core re-exports this module as
//! `supercli_core::hash`.
//!
//! Web-safe: compiles for `wasm32-unknown-unknown` (pure computation).

/// FNV-1a over the UTF-8 bytes — same constants as the Swift port.
pub fn fnv1a(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_empty_is_offset_basis() {
        // FNV-1a offset basis with no input.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn fnv1a_known_answer() {
        // Known-answer for "a": basis ^ 'a' then * prime.
        let expected = (0xcbf2_9ce4_8422_2325u64 ^ 0x61).wrapping_mul(0x0000_0100_0000_01b3);
        assert_eq!(fnv1a("a"), expected);
    }

    #[test]
    fn fnv1a_is_deterministic() {
        // Moved from launch_config's stable_hash tests.
        assert_eq!(fnv1a("hello"), fnv1a("hello"));
        assert_ne!(fnv1a("hello"), fnv1a("world"));
    }

    #[test]
    fn fnv1a_hello_known_value() {
        // Moved from launch_config's stable_hash tests.
        // FNV-1a 64-bit of "hello" — verifies the algorithm, not just
        // determinism.
        assert_eq!(fnv1a("hello"), 0xa430d84680aabd0b);
    }
}
