//! Root revision value.

use crate::InvariantError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(u64);

impl Revision {
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
            .ok_or(InvariantError::RevisionOverflow)
    }
}

impl TryFrom<u64> for Revision {
    type Error = InvariantError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvariantError::RevisionZero);
        }
        Ok(Self(value))
    }
}
