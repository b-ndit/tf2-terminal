use keyring::Entry;

use crate::error::AppResult;

const SERVICE: &str = "tf2-terminal";

/// Thin wrapper over the OS keychain (Secret Service / Keychain / Credential
/// Manager via `keyring`). Secrets (Steam API key, backpack.tf token) live
/// here only — never in SQLite, config files, or logs.
pub struct Keychain;

impl Keychain {
    pub fn set(key: &str, secret: &str) -> AppResult<()> {
        let entry = Entry::new(SERVICE, key)?;
        entry.set_password(secret)?;
        Ok(())
    }

    /// Returns `Ok(None)` if no secret is stored for `key`.
    pub fn get(key: &str) -> AppResult<Option<String>> {
        let entry = Entry::new(SERVICE, key)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// No-ops if no secret is stored for `key`.
    pub fn delete(key: &str) -> AppResult<()> {
        let entry = Entry::new(SERVICE, key)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

pub mod keys {
    pub const STEAM_API_KEY: &str = "steam_api_key";
    pub const BACKPACK_TF_TOKEN: &str = "backpack_tf_token";
    pub const DISCORD_WEBHOOK_URL: &str = "discord_webhook_url";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the keychain wrapper against whatever credential store the
    /// `keyring` crate resolves on this OS. Skipped (passes trivially) in
    /// headless CI environments with no Secret Service / keychain daemon.
    #[test]
    fn set_get_delete_roundtrip() {
        let test_key = "test_roundtrip_key";
        let Ok(()) = Keychain::set(test_key, "test-secret-value") else {
            eprintln!("skipping: no OS keychain backend available in this environment");
            return;
        };

        assert_eq!(
            Keychain::get(test_key).unwrap(),
            Some("test-secret-value".to_string())
        );

        Keychain::delete(test_key).unwrap();
        assert_eq!(Keychain::get(test_key).unwrap(), None);
    }
}
