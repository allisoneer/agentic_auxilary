//! Strict JSON-RPC envelope types.

use crate::RequestId;
use crate::ResponseId;
use crate::RpcError;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;
use serde::de::DeserializeOwned;
use serde::ser;
use serde::ser::SerializeStruct;
use serde_json::Value;
use std::fmt;
use std::marker::PhantomData;

/// The required JSON-RPC 2.0 version marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VersionVisitor;

        impl de::Visitor<'_> for VersionVisitor {
            type Value = JsonRpcVersion;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("the JSON-RPC version string \"2.0\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value == "2.0" {
                    Ok(JsonRpcVersion)
                } else {
                    Err(E::invalid_value(de::Unexpected::Str(value), &self))
                }
            }
        }

        deserializer.deserialize_str(VersionVisitor)
    }
}

/// A JSON-RPC request with optional named-object parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcRequest<P> {
    /// Required JSON-RPC 2.0 marker.
    pub jsonrpc: JsonRpcVersion,
    /// Correlation-only request ID.
    pub id: RequestId,
    /// Open method name.
    pub method: String,
    /// Optional parameters that must serialize as an object.
    pub params: Option<P>,
}

/// A JSON-RPC notification with optional named-object parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcNotification<P> {
    /// Required JSON-RPC 2.0 marker.
    pub jsonrpc: JsonRpcVersion,
    /// Open method name.
    pub method: String,
    /// Optional parameters that must serialize as an object.
    pub params: Option<P>,
}

/// A JSON-RPC response with exactly one success or error payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcResponse<R> {
    /// Required JSON-RPC 2.0 marker.
    pub jsonrpc: JsonRpcVersion,
    /// Required response ID.
    pub id: ResponseId,
    /// Exactly one top-level result or error payload.
    pub payload: RpcResponsePayload<R>,
}

/// The mutually exclusive JSON-RPC response payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcResponsePayload<R> {
    /// A successful result.
    Success(R),
    /// A peer-visible error.
    Error(RpcError),
}

#[derive(Debug)]
enum Field {
    Jsonrpc,
    Id,
    Method,
    Params,
    Result,
    Error,
    Unknown,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FieldVisitor;

        impl de::Visitor<'_> for FieldVisitor {
            type Value = Field;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON-RPC field name")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(match value {
                    "jsonrpc" => Field::Jsonrpc,
                    "id" => Field::Id,
                    "method" => Field::Method,
                    "params" => Field::Params,
                    "result" => Field::Result,
                    "error" => Field::Error,
                    _ => Field::Unknown,
                })
            }
        }

        deserializer.deserialize_identifier(FieldVisitor)
    }
}

fn object_params<P, E>(params: &P) -> Result<Value, E>
where
    P: Serialize,
    E: ser::Error,
{
    let value = serde_json::to_value(params).map_err(E::custom)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(E::custom("params must serialize as a JSON object"))
    }
}

fn deserialize_object_params<P, E>(value: Value) -> Result<P, E>
where
    P: DeserializeOwned,
    E: de::Error,
{
    if !value.is_object() {
        return Err(E::custom("params must be a JSON object"));
    }
    serde_json::from_value(value).map_err(E::custom)
}

fn validate_success_id(id: &ResponseId) -> Result<(), &'static str> {
    if matches!(id, ResponseId::Null) {
        Err("success responses require a string ID")
    } else {
        Ok(())
    }
}

impl<P> Serialize for RpcRequest<P>
where
    P: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state =
            serializer.serialize_struct("RpcRequest", 3 + usize::from(self.params.is_some()))?;
        state.serialize_field("jsonrpc", &self.jsonrpc)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("method", &self.method)?;
        if let Some(params) = &self.params {
            state.serialize_field("params", &object_params::<_, S::Error>(params)?)?;
        }
        state.end()
    }
}

impl<'de, P> Deserialize<'de> for RpcRequest<P>
where
    P: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RequestVisitor<P>(PhantomData<P>);

        impl<'de, P> de::Visitor<'de> for RequestVisitor<P>
        where
            P: DeserializeOwned,
        {
            type Value = RpcRequest<P>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON-RPC request object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut jsonrpc = None;
                let mut id = None;
                let mut method = None;
                let mut params = None;

                while let Some(field) = map.next_key()? {
                    match field {
                        Field::Jsonrpc => {
                            if jsonrpc.is_some() {
                                return Err(de::Error::duplicate_field("jsonrpc"));
                            }
                            jsonrpc = Some(map.next_value()?);
                        }
                        Field::Id => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        Field::Method => {
                            if method.is_some() {
                                return Err(de::Error::duplicate_field("method"));
                            }
                            method = Some(map.next_value()?);
                        }
                        Field::Params => {
                            if params.is_some() {
                                return Err(de::Error::duplicate_field("params"));
                            }
                            let value = map.next_value()?;
                            params = Some(deserialize_object_params(value)?);
                        }
                        Field::Result => {
                            return Err(de::Error::unknown_field("result", REQUEST_FIELDS));
                        }
                        Field::Error => {
                            return Err(de::Error::unknown_field("error", REQUEST_FIELDS));
                        }
                        Field::Unknown => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(RpcRequest {
                    jsonrpc: jsonrpc.ok_or_else(|| de::Error::missing_field("jsonrpc"))?,
                    id: id.ok_or_else(|| de::Error::missing_field("id"))?,
                    method: method.ok_or_else(|| de::Error::missing_field("method"))?,
                    params,
                })
            }
        }

        deserializer.deserialize_struct("RpcRequest", REQUEST_FIELDS, RequestVisitor(PhantomData))
    }
}

impl<P> Serialize for RpcNotification<P>
where
    P: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer
            .serialize_struct("RpcNotification", 2 + usize::from(self.params.is_some()))?;
        state.serialize_field("jsonrpc", &self.jsonrpc)?;
        state.serialize_field("method", &self.method)?;
        if let Some(params) = &self.params {
            state.serialize_field("params", &object_params::<_, S::Error>(params)?)?;
        }
        state.end()
    }
}

impl<'de, P> Deserialize<'de> for RpcNotification<P>
where
    P: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NotificationVisitor<P>(PhantomData<P>);

        impl<'de, P> de::Visitor<'de> for NotificationVisitor<P>
        where
            P: DeserializeOwned,
        {
            type Value = RpcNotification<P>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON-RPC notification object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut jsonrpc = None;
                let mut method = None;
                let mut params = None;

                while let Some(field) = map.next_key()? {
                    match field {
                        Field::Jsonrpc => {
                            if jsonrpc.is_some() {
                                return Err(de::Error::duplicate_field("jsonrpc"));
                            }
                            jsonrpc = Some(map.next_value()?);
                        }
                        Field::Method => {
                            if method.is_some() {
                                return Err(de::Error::duplicate_field("method"));
                            }
                            method = Some(map.next_value()?);
                        }
                        Field::Params => {
                            if params.is_some() {
                                return Err(de::Error::duplicate_field("params"));
                            }
                            let value = map.next_value()?;
                            params = Some(deserialize_object_params(value)?);
                        }
                        Field::Id => {
                            return Err(de::Error::unknown_field("id", NOTIFICATION_FIELDS));
                        }
                        Field::Result => {
                            return Err(de::Error::unknown_field("result", NOTIFICATION_FIELDS));
                        }
                        Field::Error => {
                            return Err(de::Error::unknown_field("error", NOTIFICATION_FIELDS));
                        }
                        Field::Unknown => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(RpcNotification {
                    jsonrpc: jsonrpc.ok_or_else(|| de::Error::missing_field("jsonrpc"))?,
                    method: method.ok_or_else(|| de::Error::missing_field("method"))?,
                    params,
                })
            }
        }

        deserializer.deserialize_struct(
            "RpcNotification",
            NOTIFICATION_FIELDS,
            NotificationVisitor(PhantomData),
        )
    }
}

impl<R> Serialize for RpcResponse<R>
where
    R: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if matches!(self.payload, RpcResponsePayload::Success(_)) {
            validate_success_id(&self.id).map_err(ser::Error::custom)?;
        }

        let mut state = serializer.serialize_struct("RpcResponse", 3)?;
        state.serialize_field("jsonrpc", &self.jsonrpc)?;
        state.serialize_field("id", &self.id)?;
        match &self.payload {
            RpcResponsePayload::Success(result) => state.serialize_field("result", result)?,
            RpcResponsePayload::Error(error) => state.serialize_field("error", error)?,
        }
        state.end()
    }
}

impl<'de, R> Deserialize<'de> for RpcResponse<R>
where
    R: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ResponseVisitor<R>(PhantomData<R>);

        impl<'de, R> de::Visitor<'de> for ResponseVisitor<R>
        where
            R: Deserialize<'de>,
        {
            type Value = RpcResponse<R>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON-RPC response object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut jsonrpc = None;
                let mut id = None;
                let mut result = None;
                let mut error = None;

                while let Some(field) = map.next_key()? {
                    match field {
                        Field::Jsonrpc => {
                            if jsonrpc.is_some() {
                                return Err(de::Error::duplicate_field("jsonrpc"));
                            }
                            jsonrpc = Some(map.next_value()?);
                        }
                        Field::Id => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        Field::Result => {
                            if result.is_some() {
                                return Err(de::Error::duplicate_field("result"));
                            }
                            result = Some(map.next_value()?);
                        }
                        Field::Error => {
                            if error.is_some() {
                                return Err(de::Error::duplicate_field("error"));
                            }
                            error = Some(map.next_value()?);
                        }
                        Field::Method => {
                            return Err(de::Error::unknown_field("method", RESPONSE_FIELDS));
                        }
                        Field::Params => {
                            return Err(de::Error::unknown_field("params", RESPONSE_FIELDS));
                        }
                        Field::Unknown => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let jsonrpc = jsonrpc.ok_or_else(|| de::Error::missing_field("jsonrpc"))?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                let payload = match (result, error) {
                    (Some(result), None) => {
                        validate_success_id(&id).map_err(de::Error::custom)?;
                        RpcResponsePayload::Success(result)
                    }
                    (None, Some(error)) => RpcResponsePayload::Error(error),
                    (Some(_), Some(_)) => {
                        return Err(de::Error::custom(
                            "result and error fields are mutually exclusive",
                        ));
                    }
                    (None, None) => {
                        return Err(de::Error::custom(
                            "response requires exactly one result or error field",
                        ));
                    }
                };

                Ok(RpcResponse {
                    jsonrpc,
                    id,
                    payload,
                })
            }
        }

        deserializer.deserialize_struct(
            "RpcResponse",
            RESPONSE_FIELDS,
            ResponseVisitor(PhantomData),
        )
    }
}

const REQUEST_FIELDS: &[&str] = &["jsonrpc", "id", "method", "params"];
const NOTIFICATION_FIELDS: &[&str] = &["jsonrpc", "method", "params"];
const RESPONSE_FIELDS: &[&str] = &["jsonrpc", "id", "result", "error"];

#[cfg(test)]
mod tests {
    use super::JsonRpcVersion;
    use super::RpcNotification;
    use super::RpcRequest;
    use super::RpcResponse;
    use super::RpcResponsePayload;
    use crate::ErrorCode;
    use crate::RequestId;
    use crate::ResponseId;
    use crate::RpcError;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn json_rpc_version_serializes_as_two_point_zero() {
        let serialized = serde_json::to_string(&JsonRpcVersion).expect("serialize marker");
        assert_eq!(serialized, r#""2.0""#);
    }

    #[test]
    fn json_rpc_version_accepts_only_two_point_zero_string() {
        assert!(serde_json::from_str::<JsonRpcVersion>(r#""2.0""#).is_ok());
        assert!(serde_json::from_str::<JsonRpcVersion>(r#""1.0""#).is_err());
        assert!(serde_json::from_str::<JsonRpcVersion>("2.0").is_err());
        assert!(serde_json::from_str::<JsonRpcVersion>("null").is_err());
    }

    #[test]
    fn requests_require_string_ids_and_object_params() {
        let request: RpcRequest<Value> = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":"request-1","method":"items.list","params":{},"future":true}"#,
        )
        .expect("valid request with additive field");
        assert_eq!(request.id, RequestId("request-1".to_string()));
        assert_eq!(request.params, Some(json!({})));

        for invalid in [
            r#"{"jsonrpc":"2.0","id":1,"method":"items.list"}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"items.list","params":null}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"items.list","params":[]}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"items.list","params":1}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"items.list","result":{}}"#,
            r#"{"jsonrpc":"2.0","id":"a","method":"items.list","error":{}}"#,
            r#"{"jsonrpc":"2.0","id":"a","id":"b","method":"items.list"}"#,
        ] {
            assert!(
                serde_json::from_str::<RpcRequest<Value>>(invalid).is_err(),
                "accepted {invalid}"
            );
        }

        let invalid_serialization = RpcRequest {
            jsonrpc: JsonRpcVersion,
            id: RequestId("request-1".to_string()),
            method: "items.list".to_string(),
            params: Some(json!([])),
        };
        assert!(serde_json::to_value(invalid_serialization).is_err());
    }

    #[test]
    fn notifications_reject_reserved_cross_kind_fields() {
        let valid: RpcNotification<Value> = serde_json::from_str(
            r#"{"jsonrpc":"2.0","method":"items.changed","params":{},"future":true}"#,
        )
        .expect("valid notification with additive field");
        assert_eq!(valid.params, Some(json!({})));

        for invalid in [
            r#"{"jsonrpc":"2.0","method":"items.changed","id":"a"}"#,
            r#"{"jsonrpc":"2.0","method":"items.changed","result":{}}"#,
            r#"{"jsonrpc":"2.0","method":"items.changed","error":{}}"#,
        ] {
            assert!(
                serde_json::from_str::<RpcNotification<Value>>(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn responses_enforce_payload_exclusivity_and_id_semantics() {
        let success: RpcResponse<Value> = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":"a","result":{"ok":true},"future":true}"#,
        )
        .expect("valid success response");
        assert!(matches!(success.payload, RpcResponsePayload::Success(_)));

        let error: RpcResponse<Value> = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}"#,
        )
        .expect("uncorrelatable error response");
        assert_eq!(error.id, ResponseId::Null);

        for invalid in [
            r#"{"jsonrpc":"2.0","id":"a","result":{},"error":{"code":-32603,"message":"x"}}"#,
            r#"{"jsonrpc":"2.0","id":"a"}"#,
            r#"{"jsonrpc":"2.0","result":{}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":null,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":"a","result":{},"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":"a","result":{},"method":"x"}"#,
            r#"{"jsonrpc":"2.0","id":"a","result":{},"params":{}}"#,
        ] {
            assert!(
                serde_json::from_str::<RpcResponse<Value>>(invalid).is_err(),
                "accepted {invalid}"
            );
        }

        let null_success = RpcResponse {
            jsonrpc: JsonRpcVersion,
            id: ResponseId::Null,
            payload: RpcResponsePayload::Success(json!({})),
        };
        assert!(serde_json::to_value(null_success).is_err());
    }

    #[test]
    fn response_serialization_emits_exactly_one_payload_field() {
        let success = RpcResponse {
            jsonrpc: JsonRpcVersion,
            id: ResponseId::Request(RequestId("a".to_string())),
            payload: RpcResponsePayload::Success(json!({"ok": true})),
        };
        let success_value = serde_json::to_value(success).expect("serialize success");
        assert!(success_value.get("result").is_some());
        assert!(success_value.get("error").is_none());

        let error = RpcResponse::<Value> {
            jsonrpc: JsonRpcVersion,
            id: ResponseId::Null,
            payload: RpcResponsePayload::Error(RpcError {
                code: ErrorCode(-32700),
                message: "Parse error".to_string(),
                data: None,
            }),
        };
        let error_value = serde_json::to_value(error).expect("serialize error");
        assert!(error_value.get("result").is_none());
        assert!(error_value.get("error").is_some());
    }
}
