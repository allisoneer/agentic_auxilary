use crate::http::HttpClient;

pub mod global;
pub mod session;

pub const APPROVED_ROUTE_COUNT: usize = 4;
pub const APPROVED_ROUTES: [(&str, &str); APPROVED_ROUTE_COUNT] = [
    ("POST", "/session"),
    ("GET", "/session/:sessionID"),
    ("POST", "/session/:sessionID/command"),
    ("GET", "/global/health"),
];

#[derive(Clone)]
pub struct LegacyClient {
    http: HttpClient,
}

impl LegacyClient {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub fn global(&self) -> global::GlobalApi {
        global::GlobalApi::new(self.http.clone())
    }

    pub fn session(&self) -> session::SessionApi {
        session::SessionApi::new(self.http.clone())
    }

    pub async fn global_health(&self) -> crate::error::Result<crate::http::misc::HealthInfo> {
        self.global().health().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_exactly_four_approved_routes() {
        assert_eq!(APPROVED_ROUTES.len(), APPROVED_ROUTE_COUNT);
        assert_eq!(APPROVED_ROUTES[0], ("POST", "/session"));
        assert_eq!(APPROVED_ROUTES[1], ("GET", "/session/:sessionID"));
        assert_eq!(APPROVED_ROUTES[2], ("POST", "/session/:sessionID/command"));
        assert_eq!(APPROVED_ROUTES[3], ("GET", "/global/health"));
    }
}
