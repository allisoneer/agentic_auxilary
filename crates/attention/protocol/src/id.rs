//! Opaque wire identifier types.

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;
use std::fmt;

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
    };
}

string_id!(
    /// A correlation-only JSON-RPC request identifier.
    RequestId
);
string_id!(
    /// An opaque server identity.
    ServerId
);
string_id!(
    /// An opaque logical stream identity.
    StreamId
);
string_id!(
    /// An opaque server-process boot identity.
    BootId
);
string_id!(
    /// An opaque stream cursor.
    Cursor
);

/// A required JSON-RPC response ID, which may be null for uncorrelatable errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResponseId {
    /// No request ID could be correlated.
    Null,
    /// The response correlates to a string request ID.
    Request(RequestId),
}

impl Serialize for ResponseId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Request(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ResponseIdVisitor;

        impl de::Visitor<'_> for ResponseIdVisitor {
            type Value = ResponseId;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string request ID or null")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ResponseId::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ResponseId::Null)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ResponseId::Request(RequestId(value.to_owned())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ResponseId::Request(RequestId(value)))
            }
        }

        deserializer.deserialize_any(ResponseIdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::RequestId;
    use super::ResponseId;

    #[test]
    fn request_id_accepts_only_strings_without_content_rules() {
        assert_eq!(
            serde_json::from_str::<RequestId>(r#""""#).expect("empty string remains opaque"),
            RequestId(String::new())
        );
        assert!(serde_json::from_str::<RequestId>("1").is_err());
        assert!(serde_json::from_str::<RequestId>("null").is_err());
    }

    #[test]
    fn response_id_accepts_only_null_or_string() {
        assert_eq!(
            serde_json::from_str::<ResponseId>("null").expect("null response ID"),
            ResponseId::Null
        );
        assert_eq!(
            serde_json::from_str::<ResponseId>(r#""request-1""#).expect("string response ID"),
            ResponseId::Request(RequestId("request-1".to_string()))
        );
        assert!(serde_json::from_str::<ResponseId>("1").is_err());
        assert!(serde_json::from_str::<ResponseId>("{}").is_err());
    }
}
