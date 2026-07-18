use std::sync::OnceLock;

use keyring_core::Entry;

use crate::error::AppResult;

const SERVICE: &str = "tf2-terminal";

static STORE_INIT: OnceLock<()> = OnceLock::new();

/// Wires up the OS-appropriate credential store the first time any secret
/// operation runs, then reuses it for the rest of the process.
///
/// This intentionally bypasses `keyring` 4.x's own `v1` convenience API,
/// which has a real footgun on Linux: it lazily wires up Secret Service on
/// first use, but if that first attempt fails — no Secret Service D-Bus
/// provider reachable, e.g. a plain WSL2 install or a minimal window
/// manager with no `gnome-keyring`/`kwalletd` running, verified live —
/// `v1` marks itself "already initialized" regardless and permanently
/// returns `NoDefaultStore` for the rest of the process, with no way to
/// retry through its own API. Going straight to `keyring-core` lets us
/// fall back instead: on Linux, if Secret Service isn't reachable, try the
/// kernel keyring (`linux-keyutils-keyring-store`, no daemon required).
/// That fallback only persists `UntilReboot` (its documented persistence)
/// rather than indefinitely — a degraded mode, but strictly better than
/// the keychain being unusable for the whole session.
fn ensure_store_initialized() {
    STORE_INIT.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
                keyring_core::set_default_store(store);
                return;
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Ok(store) = windows_native_keyring_store::Store::new() {
                keyring_core::set_default_store(store);
                return;
            }
        }
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ))]
        {
            if let Ok(store) = zbus_secret_service_keyring_store::Store::new() {
                keyring_core::set_default_store(store);
                return;
            }
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(store) = linux_keyutils_keyring_store::Store::new() {
                tracing::warn!(
                    "no OS Secret Service reachable; falling back to the Linux kernel keyring \
                     (secrets stored this way are cleared on reboot)"
                );
                keyring_core::set_default_store(store);
                return;
            }
        }
        tracing::error!("no credential store available on this platform; secrets cannot be saved");
    });
}

/// Thin wrapper over the OS keychain (Secret Service / Keychain / Credential
/// Manager, or the Linux kernel keyring as a fallback — see
/// `ensure_store_initialized`). Secrets (Steam API key, backpack.tf token)
/// live here only — never in SQLite, config files, or logs.
pub struct Keychain;

impl Keychain {
    pub fn set(key: &str, secret: &str) -> AppResult<()> {
        ensure_store_initialized();
        let entry = Entry::new(SERVICE, key)?;
        entry.set_password(secret)?;
        Ok(())
    }

    /// Returns `Ok(None)` if no secret is stored for `key`.
    pub fn get(key: &str) -> AppResult<Option<String>> {
        ensure_store_initialized();
        let entry = Entry::new(SERVICE, key)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// No-ops if no secret is stored for `key`.
    pub fn delete(key: &str) -> AppResult<()> {
        ensure_store_initialized();
        let entry = Entry::new(SERVICE, key)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
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

    /// Exercises the keychain wrapper against whatever credential store
    /// `ensure_store_initialized` resolves on this OS — real Secret
    /// Service if one's reachable, the kernel-keyring fallback on Linux
    /// otherwise. Skipped (passes trivially) only if neither is available
    /// (e.g. an exotic sandbox with no keyctl support at all).
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
