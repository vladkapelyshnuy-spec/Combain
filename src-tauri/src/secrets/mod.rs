//! Secure storage for provider API keys, backed by the OS credential store
//! (Windows Credential Manager, macOS Keychain, Linux Secret Service) via the
//! `keyring` crate. Secrets never touch SQLite, logs, or the frontend.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::AppError;

const SERVICE_NAME: &str = "combain-studio";

/// Cache of `Entry` handles keyed by alias.
///
/// Real OS credential stores are keyed by `(service, user)` regardless of how
/// many `Entry` instances address them, but the mock backend used in tests
/// allocates fresh, independent state on every `Entry::new()` call. Reusing
/// one `Entry` per alias keeps behavior consistent across both.
fn entry_cache() -> &'static Mutex<HashMap<String, Arc<keyring::Entry>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<keyring::Entry>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn entry_for(alias: &str) -> Result<Arc<keyring::Entry>, AppError> {
    let mut cache = entry_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(entry) = cache.get(alias) {
        return Ok(entry.clone());
    }

    let entry = keyring::Entry::new(SERVICE_NAME, alias).map_err(|error| {
        AppError::internal(format!("failed to access credential store: {error}"))
    })?;
    let entry = Arc::new(entry);
    cache.insert(alias.to_string(), entry.clone());
    Ok(entry)
}

/// Store (or overwrite) the secret for `alias`.
pub fn set_secret(alias: &str, value: &str) -> Result<(), AppError> {
    entry_for(alias)?
        .set_password(value)
        .map_err(|error| AppError::internal(format!("failed to store credential: {error}")))
}

/// Look up the secret for `alias`. Returns `None` if no credential exists.
pub fn get_secret(alias: &str) -> Result<Option<String>, AppError> {
    match entry_for(alias)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::internal(format!(
            "failed to read credential: {error}"
        ))),
    }
}

/// Delete the secret for `alias`. A missing credential is not an error.
pub fn delete_secret(alias: &str) -> Result<(), AppError> {
    match entry_for(alias)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::internal(format!(
            "failed to delete credential: {error}"
        ))),
    }
}

/// Swap in the in-memory mock credential store so tests never touch the real
/// OS credential manager. Shared across test modules in this crate (e.g.
/// `commands::provider_profile_commands::tests`) since the underlying
/// builder is a process-wide global.
#[cfg(test)]
pub(crate) fn install_mock_backend() {
    use std::sync::Once;
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn use_mock_backend() {
        install_mock_backend();
    }

    #[test]
    fn round_trips_a_secret() {
        use_mock_backend();
        let alias = uuid::Uuid::new_v4().to_string();

        set_secret(&alias, "sk-test-123").expect("set");
        let loaded = get_secret(&alias).expect("get");

        assert_eq!(loaded, Some("sk-test-123".to_string()));
    }

    #[test]
    fn missing_secret_returns_none() {
        use_mock_backend();
        let alias = uuid::Uuid::new_v4().to_string();

        let loaded = get_secret(&alias).expect("get");
        assert_eq!(loaded, None);
    }

    #[test]
    fn delete_removes_secret() {
        use_mock_backend();
        let alias = uuid::Uuid::new_v4().to_string();

        set_secret(&alias, "sk-test-123").expect("set");
        delete_secret(&alias).expect("delete");

        let loaded = get_secret(&alias).expect("get");
        assert_eq!(loaded, None);
    }

    #[test]
    fn delete_missing_secret_is_ok() {
        use_mock_backend();
        let alias = uuid::Uuid::new_v4().to_string();

        delete_secret(&alias).expect("delete missing is ok");
    }

    #[test]
    fn overwriting_a_secret_replaces_the_value() {
        use_mock_backend();
        let alias = uuid::Uuid::new_v4().to_string();

        set_secret(&alias, "first").expect("set first");
        set_secret(&alias, "second").expect("set second");

        let loaded = get_secret(&alias).expect("get");
        assert_eq!(loaded, Some("second".to_string()));
    }
}
