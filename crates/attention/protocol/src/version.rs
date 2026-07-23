//! Attention protocol version types.

use serde::Deserialize;
use serde::Serialize;

/// A negotiated Attention protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(pub u32);

/// The first Attention protocol version.
pub const PROTOCOL_V1: ProtocolVersion = ProtocolVersion(1);
