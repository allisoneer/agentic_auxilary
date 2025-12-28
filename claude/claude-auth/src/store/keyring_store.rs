//! Keyring-based secure storage

use super::SecureStore;
use crate::error::AuthError;
use base64::{engine::general_purpose::STANDARD, Engine};

/// Keyring-based secret storage using the system keychain
#[derive(Clone)]
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    /// Create a new keyring store for the given service name
    pub fn new(service: &str) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl SecureStore for KeyringStore {
    fn set_secret(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        let entry = keyring::Entry::new(&self.service, name)
            .map_err(|e| AuthError::Storage(e.to_string()))?;
        let encoded = STANDARD.encode(value);
        entry
            .set_password(&encoded)
            .map_err(|e| AuthError::Storage(e.to_string()))
    }

    fn get_secret(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        let entry = keyring::Entry::new(&self.service, name)
            .map_err(|e| AuthError::Storage(e.to_string()))?;
        match entry.get_password() {
            Ok(s) => Ok(STANDARD.decode(s).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn delete_secret(&self, name: &str) -> Result<(), AuthError> {
        let entry = keyring::Entry::new(&self.service, name)
            .map_err(|e| AuthError::Storage(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }
}
