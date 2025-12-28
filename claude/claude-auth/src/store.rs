//! Secure storage trait and implementations

mod keyring_store;
mod xdg_store;

pub use keyring_store::KeyringStore;
pub use xdg_store::XdgFileStore;

use crate::error::AuthError;

/// Trait for secure secret storage
pub trait SecureStore: Send + Sync + Clone + 'static {
    /// Store a secret value
    fn set_secret(&self, name: &str, value: &[u8]) -> Result<(), AuthError>;
    /// Retrieve a secret value
    fn get_secret(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError>;
    /// Delete a secret value
    fn delete_secret(&self, name: &str) -> Result<(), AuthError>;
}
