//! Claude Auth - OAuth PKCE and API key authentication

pub mod api_key;
pub mod callback;
pub mod error;
pub mod oauth;
pub mod pkce;
pub mod provider;
pub mod store;

pub use api_key::ApiKeyManager;
pub use error::AuthError;
pub use oauth::OAuthPkceManager;
pub use provider::{AuthMode, TokenProvider};
pub use store::{KeyringStore, SecureStore, XdgFileStore};
