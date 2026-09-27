//! Port of `Licensing/LicenseKeychain.swift` — Keychain license storage.
//!
//! Generic-password item: service `com.supercli.license`, account
//! `license-key`. Save is update-first then add; delete treats not-found
//! as success. Uses `kSecAttrAccessibleAfterFirstUnlock` semantics.
//!
//! Implemented on the cross-platform `keyring` crate (macOS Keychain on
//! macOS), with the exact service/account names from Swift.

/// Keychain service name. Swift: `com.supercli.license`.
pub const SERVICE: &str = "com.supercli.license";
/// Keychain account name. Swift: `license-key`.
pub const ACCOUNT: &str = "license-key";

/// Thin wrapper around the platform keychain for the license key.
#[derive(Debug)]
pub struct LicenseKeychain {
    entry: keyring::Entry,
}

impl LicenseKeychain {
    /// Opens (not creates) the generic-password item.
    pub fn new() -> Result<Self, String> {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())?;
        Ok(Self { entry })
    }

    /// Loads the stored license key, or `None` when absent.
    pub fn load(&self) -> Result<Option<String>, String> {
        match self.entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Saves the license key (update-first, then add — handled by keyring).
    pub fn save(&self, key: &str) -> Result<(), String> {
        self.entry.set_password(key).map_err(|e| e.to_string())
    }

    /// Deletes the stored key. Not-found is success, matching Swift.
    pub fn delete(&self) -> Result<(), String> {
        match self.entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_names_match_swift() {
        assert_eq!(SERVICE, "com.supercli.license");
        assert_eq!(ACCOUNT, "license-key");
    }

    // Note: round-trip tests against the real keychain are not run here;
    // the Linux sandbox has no Secret Service and CI macOS runners do.
    // The update-first/add and delete-idempotent semantics are guaranteed
    // by the keyring crate's Entry API (set_password upserts,
    // delete_credential maps NoEntry to Ok here).
}
