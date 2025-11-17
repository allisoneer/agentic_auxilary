use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String, // "ephemeral"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
}

impl CacheControl {
    #[must_use]
    pub fn ephemeral_5m() -> Self {
        Self {
            kind: "ephemeral".into(),
            ttl: Some(CacheTtl::FiveMinutes),
        }
    }

    #[must_use]
    pub fn ephemeral_1h() -> Self {
        Self {
            kind: "ephemeral".into(),
            ttl: Some(CacheTtl::OneHour),
        }
    }

    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            kind: "ephemeral".into(),
            ttl: None,
        }
    }
}

/// Validate that when mixing TTLs, `OneHour` entries appear before `FiveMinutes`.
#[must_use]
pub fn validate_mixed_ttl_order(block_ttls: impl IntoIterator<Item = CacheTtl>) -> bool {
    let mut seen_5m = false;
    for ttl in block_ttls {
        match ttl {
            CacheTtl::OneHour if seen_5m => return false, // 1h after 5m → invalid
            CacheTtl::FiveMinutes => seen_5m = true,
            CacheTtl::OneHour => {}
        }
    }
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_ser_de() {
        let s = serde_json::to_string(&CacheTtl::FiveMinutes).unwrap();
        assert_eq!(s, r#""5m""#);
        let t: CacheTtl = serde_json::from_str(r#""1h""#).unwrap();
        assert_eq!(t, CacheTtl::OneHour);
    }

    #[test]
    fn cache_control_ser() {
        let cc = CacheControl::ephemeral_5m();
        let s = serde_json::to_string(&cc).unwrap();
        assert!(s.contains(r#""type":"ephemeral""#));
        assert!(s.contains(r#""ttl":"5m""#));
    }

    #[test]
    fn ordering_valid() {
        assert!(validate_mixed_ttl_order([
            CacheTtl::OneHour,
            CacheTtl::FiveMinutes
        ]));
        assert!(validate_mixed_ttl_order([CacheTtl::FiveMinutes]));
        assert!(validate_mixed_ttl_order([CacheTtl::OneHour]));
        assert!(!validate_mixed_ttl_order([
            CacheTtl::FiveMinutes,
            CacheTtl::OneHour
        ]));
    }
}
