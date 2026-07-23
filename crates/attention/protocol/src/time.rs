//! Canonical wire timestamp type.

use crate::error::ProtocolError;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;

/// A UTC timestamp in canonical RFC3339 form with six fractional digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WireTimestamp(DateTime<Utc>);

impl WireTimestamp {
    /// Constructs a wire timestamp without silently truncating sub-microsecond precision.
    pub fn from_datetime(value: DateTime<Utc>) -> Result<Self, ProtocolError> {
        if !value.timestamp_subsec_nanos().is_multiple_of(1_000) {
            return Err(ProtocolError::TimestampValidation(
                "timestamp has sub-microsecond precision".to_string(),
            ));
        }
        Ok(Self(value))
    }

    /// Parses only the canonical wire representation.
    pub fn parse(value: &str) -> Result<Self, ProtocolError> {
        let parsed = DateTime::parse_from_rfc3339(value).map_err(|error| {
            ProtocolError::TimestampValidation(format!("invalid RFC3339 timestamp: {error}"))
        })?;
        let timestamp = Self::from_datetime(parsed.with_timezone(&Utc))?;
        if timestamp.to_canonical_string() != value {
            return Err(ProtocolError::TimestampValidation(
                "timestamp is not in canonical UTC microsecond form".to_string(),
            ));
        }
        Ok(timestamp)
    }

    /// Returns the underlying UTC instant.
    pub const fn as_datetime(&self) -> &DateTime<Utc> {
        &self.0
    }

    fn to_canonical_string(&self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Micros, true)
    }
}

impl TryFrom<DateTime<Utc>> for WireTimestamp {
    type Error = ProtocolError;

    fn try_from(value: DateTime<Utc>) -> Result<Self, Self::Error> {
        Self::from_datetime(value)
    }
}

impl Serialize for WireTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_canonical_string())
    }
}

impl<'de> Deserialize<'de> for WireTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::WireTimestamp;
    use chrono::DateTime;
    use chrono::Utc;

    const CANONICAL: &str = "2026-07-22T14:00:00.123456Z";

    #[test]
    fn canonical_timestamp_round_trips() {
        let timestamp = WireTimestamp::parse(CANONICAL).expect("canonical timestamp");
        assert_eq!(
            serde_json::to_string(&timestamp).expect("serialize timestamp"),
            format!(r#""{CANONICAL}""#)
        );
    }

    #[test]
    fn parser_rejects_noncanonical_equivalent_forms() {
        for invalid in [
            "2026-07-22T14:00:00Z",
            "2026-07-22T14:00:00.123Z",
            "2026-07-22T14:00:00.123456000Z",
            "2026-07-22T14:00:00.123456+00:00",
            "2026-07-22T14:00:00.123456z",
        ] {
            assert!(WireTimestamp::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn constructor_rejects_sub_microsecond_precision() {
        let value = DateTime::parse_from_rfc3339("2026-07-22T14:00:00.123456789Z")
            .expect("valid RFC3339")
            .with_timezone(&Utc);
        assert!(WireTimestamp::from_datetime(value).is_err());
    }
}
