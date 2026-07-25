//! Canonical scalar wire values.

use crate::ProtocolError;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;

macro_rules! decimal_string {
    ($(#[$meta:meta])* $name:ident, $label:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Parses a nonzero canonical unsigned decimal string.
            pub fn parse(value: impl Into<String>) -> Result<Self, ProtocolError> {
                let value = value.into();
                if !is_canonical_nonzero_decimal(&value) {
                    return Err(ProtocolError::ValueValidation(format!(
                        "{} must be a nonzero canonical decimal string",
                        $label
                    )));
                }
                Ok(Self(value))
            }

            /// Returns the canonical decimal representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $name {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0
                    .len()
                    .cmp(&other.0.len())
                    .then_with(|| self.0.cmp(&other.0))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::parse(String::deserialize(deserializer)?).map_err(de::Error::custom)
            }
        }
    };
}

decimal_string!(
    /// A nonzero resource revision encoded as a decimal JSON string.
    Revision,
    "revision"
);
decimal_string!(
    /// A nonzero source-state version encoded as a decimal JSON string.
    SourceStateVersion,
    "source state version"
);

fn is_canonical_nonzero_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.as_bytes()[0] != b'0'
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

macro_rules! base64url_string {
    ($(#[$meta:meta])* $name:ident, $expected_bytes:expr, $label:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Parses canonical unpadded URL-safe Base64 syntax.
            pub fn parse(value: impl Into<String>) -> Result<Self, ProtocolError> {
                let value = value.into();
                validate_base64url(&value, $expected_bytes, $label)?;
                Ok(Self(value))
            }

            /// Returns the canonical encoded representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::parse(String::deserialize(deserializer)?).map_err(de::Error::custom)
            }
        }
    };
}

base64url_string!(
    /// Nonempty normalized source-order bytes in canonical unpadded URL-safe Base64.
    NormalizedSourceOrder,
    None,
    "normalized source order"
);
base64url_string!(
    /// A 32-byte delivery lease capability in canonical unpadded URL-safe Base64.
    DeliveryLeaseToken,
    Some(32),
    "delivery lease token"
);

fn validate_base64url(
    value: &str,
    expected_bytes: Option<usize>,
    label: &'static str,
) -> Result<(), ProtocolError> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() % 4 == 1
        || !bytes.iter().all(|byte| base64_value(*byte).is_some())
    {
        return Err(value_error(label));
    }

    let remainder = bytes.len() % 4;
    if (remainder == 2
        && base64_value(bytes[bytes.len() - 1]).is_none_or(|value| value & 0x0f != 0))
        || (remainder == 3
            && base64_value(bytes[bytes.len() - 1]).is_none_or(|value| value & 0x03 != 0))
    {
        return Err(value_error(label));
    }

    let decoded_length = bytes.len() / 4 * 3
        + match remainder {
            0 => 0,
            2 => 1,
            3 => 2,
            _ => return Err(value_error(label)),
        };
    if expected_bytes.is_some_and(|expected| decoded_length != expected) {
        return Err(value_error(label));
    }
    Ok(())
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn value_error(label: &'static str) -> ProtocolError {
    ProtocolError::ValueValidation(format!(
        "{label} must use canonical unpadded URL-safe Base64"
    ))
}

#[cfg(test)]
mod tests {
    use super::DeliveryLeaseToken;
    use super::NormalizedSourceOrder;
    use super::Revision;
    use super::SourceStateVersion;

    #[test]
    fn decimal_values_require_canonical_nonzero_strings() {
        for invalid in ["", "0", "01", "+1", "-1", " 1", "1 ", "1.0", "a"] {
            assert!(Revision::parse(invalid).is_err(), "accepted {invalid:?}");
            assert!(
                SourceStateVersion::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
        assert_eq!(
            Revision::parse("18446744073709551615")
                .expect("revision")
                .as_str(),
            "18446744073709551615"
        );
    }

    #[test]
    fn revisions_order_numerically_across_digit_boundaries() {
        let nine = Revision::parse("9").expect("revision");
        let ten = Revision::parse("10").expect("revision");

        assert!(nine < ten);
    }

    #[test]
    fn source_state_versions_order_numerically_across_digit_boundaries() {
        let ninety_nine = SourceStateVersion::parse("99").expect("source state version");
        let one_hundred = SourceStateVersion::parse("100").expect("source state version");

        assert!(ninety_nine < one_hundred);
    }

    #[test]
    fn base64_values_require_canonical_url_safe_syntax() {
        assert!(NormalizedSourceOrder::parse("AQ").is_ok());
        for invalid in ["", "A", "AB", "A+", "A/", "AQ=", "AQ==", "AR"] {
            assert!(
                NormalizedSourceOrder::parse(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn lease_token_requires_exactly_thirty_two_bytes() {
        let token = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";
        assert!(DeliveryLeaseToken::parse(token).is_ok());
        assert!(DeliveryLeaseToken::parse("AQ").is_err());
    }
}
