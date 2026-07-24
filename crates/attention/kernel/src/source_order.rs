//! Source ordering and optimistic authority versions.

use crate::InvariantError;
use crate::SourceReceiptId;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceStateVersion(u64);

impl SourceStateVersion {
    pub const fn initial() -> Self {
        Self(1)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn checked_increment(self) -> Result<Self, InvariantError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(InvariantError::SourceStateVersionOverflow)
    }
}

impl TryFrom<u64> for SourceStateVersion {
    type Error = InvariantError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvariantError::SourceStateVersionZero);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceComparatorDomain(String);

impl SourceComparatorDomain {
    pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InvariantError::EmptyValue {
                value: "source comparator domain",
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalizedSourceOrder(Vec<u8>);

impl NormalizedSourceOrder {
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvariantError::EmptyValue {
                value: "normalized source order",
            });
        }
        Ok(Self(value))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceOrderMode {
    Unordered,
    Ordered {
        domain: SourceComparatorDomain,
        value: Option<NormalizedSourceOrder>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceOrderComparison {
    Newer,
    Equal,
    Older,
    Incomparable,
}

impl SourceOrderMode {
    pub fn compare_to(&self, current: &Self) -> SourceOrderComparison {
        let (
            Self::Ordered {
                domain: proposed_domain,
                value: Some(proposed),
            },
            Self::Ordered {
                domain: current_domain,
                value: Some(current),
            },
        ) = (self, current)
        else {
            return SourceOrderComparison::Incomparable;
        };
        if proposed_domain != current_domain {
            return SourceOrderComparison::Incomparable;
        }
        match proposed.cmp(current) {
            Ordering::Greater => SourceOrderComparison::Newer,
            Ordering::Equal => SourceOrderComparison::Equal,
            Ordering::Less => SourceOrderComparison::Older,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedSourceAuthority {
    Absent,
    Present {
        version: SourceStateVersion,
        latest_receipt_id: SourceReceiptId,
        order: SourceOrderMode,
    },
}

impl ObservedSourceAuthority {
    pub const fn version(&self) -> Option<SourceStateVersion> {
        match self {
            Self::Absent => None,
            Self::Present { version, .. } => Some(*version),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryLimit(usize);

impl TryFrom<usize> for QueryLimit {
    type Error = InvariantError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvariantError::ZeroBound {
                value: "query limit",
            });
        }
        Ok(Self(value))
    }
}

impl QueryLimit {
    pub const fn value(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClaimLimit(usize);

impl TryFrom<usize> for ClaimLimit {
    type Error = InvariantError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvariantError::ZeroBound {
                value: "claim limit",
            });
        }
        Ok(Self(value))
    }
}

impl ClaimLimit {
    pub const fn value(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::NormalizedSourceOrder;
    use super::SourceComparatorDomain;
    use super::SourceOrderComparison;
    use super::SourceOrderMode;
    use super::SourceStateVersion;

    fn ordered(domain: &str, value: &[u8]) -> SourceOrderMode {
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new(domain).expect("valid domain"),
            value: Some(NormalizedSourceOrder::new(value).expect("valid order")),
        }
    }

    #[test]
    fn source_values_compare_only_inside_one_ordered_domain() {
        assert_eq!(
            ordered("sequence", &[2]).compare_to(&ordered("sequence", &[1])),
            SourceOrderComparison::Newer
        );
        assert_eq!(
            ordered("sequence", &[1]).compare_to(&ordered("timestamp", &[1])),
            SourceOrderComparison::Incomparable
        );
        assert_eq!(
            SourceOrderMode::Unordered.compare_to(&SourceOrderMode::Unordered),
            SourceOrderComparison::Incomparable
        );
    }

    #[test]
    fn source_version_validates_zero_and_overflow() {
        assert!(SourceStateVersion::try_from(0).is_err());
        assert!(
            SourceStateVersion::try_from(u64::MAX)
                .expect("nonzero")
                .checked_increment()
                .is_err()
        );
    }
}
