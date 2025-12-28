//! OAuth PKCE authentication manager

use crate::{
    callback::run_callback_server,
    error::AuthError,
    pkce::{code_challenge_s256, generate_code_verifier},
    provider::TokenProvider,
    store::SecureStore,
};
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tokio::sync::RwLock;
use url::Url;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://console.anthropic.com/api/oauth/token";
const REDIRECT_URI: &str = "http://127.0.0.1:19876/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference";

const REFRESH_TOKEN_KEY: &str = "oauth_refresh_token";

#[derive(Debug, Serialize, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    token_type: String,
}

struct TokenState {
    access_token: String,
    expires_at: OffsetDateTime,
}

/// OAuth PKCE authentication manager
pub struct OAuthPkceManager<S: SecureStore> {
    store: S,
    current: Arc<RwLock<Option<TokenState>>>,
}

impl<S: SecureStore> OAuthPkceManager<S> {
    /// Create a new OAuth PKCE manager with the given store
    pub fn new(store: S) -> Self {
        Self {
            store,
            current: Arc::new(RwLock::new(None)),
        }
    }

    /// Generate a 64-character hex state parameter for OAuth CSRF protection
    fn generate_state() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Ensure the user is logged in, prompting if necessary
    pub async fn ensure_logged_in(&self) -> Result<(), AuthError> {
        // Check if we have a valid token
        {
            let guard = self.current.read().await;
            if let Some(state) = guard.as_ref() {
                if state.expires_at > OffsetDateTime::now_utc() + Duration::minutes(5) {
                    return Ok(());
                }
            }
        }

        // Try refresh if we have a refresh token
        if self.store.get_secret(REFRESH_TOKEN_KEY)?.is_some()
            && self.refresh_token().await.is_ok()
        {
            return Ok(());
        }

        // Start fresh login flow
        self.start_login_flow().await
    }

    async fn start_login_flow(&self) -> Result<(), AuthError> {
        let verifier = generate_code_verifier();
        let challenge = code_challenge_s256(&verifier);
        let state = Self::generate_state();

        let mut url = Url::parse(AUTH_URL).map_err(|e| AuthError::Other(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("scope", SCOPES)
            .append_pair("state", &state);

        tracing::info!("Opening browser for OAuth login");
        open::that(url.as_str())
            .map_err(|e| AuthError::Other(format!("Failed to open browser: {e}")))?;

        let code = run_callback_server(&state).await?;

        self.exchange_code(&code, &verifier).await
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<(), AuthError> {
        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("redirect_uri", REDIRECT_URI),
                ("code_verifier", verifier),
                ("code", code),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AuthError::Token(format!("Token exchange failed: {text}")));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::Other(e.to_string()))?;

        self.store_tokens(&token_resp).await
    }

    async fn store_tokens(&self, resp: &TokenResponse) -> Result<(), AuthError> {
        if let Some(refresh) = &resp.refresh_token {
            self.store
                .set_secret(REFRESH_TOKEN_KEY, refresh.as_bytes())?;
        }

        let expires_at =
            OffsetDateTime::now_utc() + Duration::seconds(i64::try_from(resp.expires_in).unwrap_or(3600) - 60);
        *self.current.write().await = Some(TokenState {
            access_token: resp.access_token.clone(),
            expires_at,
        });

        // Spawn proactive background refresh task (5 minutes before expiry)
        let refresh_at = expires_at - Duration::minutes(5);
        let now = OffsetDateTime::now_utc();
        if refresh_at > now {
            let current = self.current.clone();
            let store = self.store.clone();
            tokio::spawn(async move {
                let dur = (refresh_at - now).whole_milliseconds().max(0) as u64;
                tracing::info!("Scheduling proactive token refresh in {} seconds", dur / 1000);
                tokio::time::sleep(std::time::Duration::from_millis(dur)).await;
                
                // Check if token is still valid (hasn't been replaced)
                {
                    let guard = current.read().await;
                    if let Some(state) = guard.as_ref() {
                        // Only refresh if this is still the same token
                        if state.expires_at != expires_at {
                            tracing::info!("Token was already refreshed, skipping scheduled refresh");
                            return;
                        }
                    }
                }
                
                // Attempt refresh
                if let Ok(Some(refresh)) = store.get_secret(REFRESH_TOKEN_KEY) {
                    if let Ok(refresh_str) = String::from_utf8(refresh) {
                        tracing::info!("Executing proactive token refresh");
                        let client = reqwest::Client::new();
                        if let Ok(resp) = client
                            .post(TOKEN_URL)
                            .form(&[
                                ("grant_type", "refresh_token"),
                                ("client_id", CLIENT_ID),
                                ("refresh_token", &refresh_str),
                            ])
                            .send()
                            .await
                        {
                            if resp.status().is_success() {
                                if let Ok(token_resp) = resp.json::<TokenResponse>().await {
                                    let new_expires_at = OffsetDateTime::now_utc() 
                                        + Duration::seconds(i64::try_from(token_resp.expires_in).unwrap_or(3600) - 60);
                                    *current.write().await = Some(TokenState {
                                        access_token: token_resp.access_token,
                                        expires_at: new_expires_at,
                                    });
                                    tracing::info!("Proactive token refresh successful");
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn refresh_token(&self) -> Result<(), AuthError> {
        let refresh = self
            .store
            .get_secret(REFRESH_TOKEN_KEY)?
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| AuthError::Token("No refresh token".into()))?;

        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT_ID),
                ("refresh_token", &refresh),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AuthError::Token("Refresh failed".into()));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AuthError::Other(e.to_string()))?;

        self.store_tokens(&token_resp).await
    }

    async fn get_access_token(&self) -> Result<Option<String>, AuthError> {
        let guard = self.current.read().await;
        Ok(guard.as_ref().map(|s| s.access_token.clone()))
    }

    /// Check if user is logged in (has refresh token)
    pub fn is_logged_in(&self) -> bool {
        self.store
            .get_secret(REFRESH_TOKEN_KEY)
            .ok()
            .flatten()
            .is_some()
    }

    /// Log out by clearing tokens
    pub fn logout(&self) -> Result<(), AuthError> {
        self.store.delete_secret(REFRESH_TOKEN_KEY)
    }
}

impl<S: SecureStore> TokenProvider for OAuthPkceManager<S> {
    fn get_headers(&self) -> Result<HeaderMap, AuthError> {
        let mut headers = HeaderMap::new();

        // Use tokio's current runtime to get token synchronously
        let token = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.get_access_token())
        })?;

        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|e| AuthError::Config(e.to_string()))?,
            );
        }
        Ok(headers)
    }

    fn has_credentials(&self) -> bool {
        self.is_logged_in()
    }
}
