use crate::error::Error;
use crate::error::Result;
use git2::Reference;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

const ADMIN_PREFIX: &str = "gwt-";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchName(String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AdminName(String);

impl BranchName {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        validate_branch_name(&name)?;
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn encode_admin_name(&self) -> AdminName {
        let mut encoded = String::with_capacity(ADMIN_PREFIX.len() + (self.0.len() * 2));
        encoded.push_str(ADMIN_PREFIX);
        for byte in self.0.as_bytes() {
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
        AdminName(encoded)
    }
}

impl AdminName {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        validate_admin_name(&name)?;
        Ok(Self(name))
    }

    pub fn decode_branch_name(&self) -> Result<BranchName> {
        let Some(payload) = self.0.strip_prefix(ADMIN_PREFIX) else {
            return Err(Error::InvalidAdminEncoding(self.0.clone()));
        };
        if payload.len() % 2 != 0 {
            return Err(Error::InvalidAdminEncoding(self.0.clone()));
        }

        let mut bytes = Vec::with_capacity(payload.len() / 2);
        for chunk in payload.as_bytes().chunks_exact(2) {
            let high =
                hex_value(chunk[0]).ok_or_else(|| Error::InvalidAdminEncoding(self.0.clone()))?;
            let low =
                hex_value(chunk[1]).ok_or_else(|| Error::InvalidAdminEncoding(self.0.clone()))?;
            bytes.push((high << 4) | low);
        }

        let decoded =
            String::from_utf8(bytes).map_err(|_| Error::InvalidAdminEncoding(self.0.clone()))?;
        BranchName::new(decoded)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for BranchName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for BranchName {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<&str> for AdminName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for AdminName {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for AdminName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('\0') {
        return Err(Error::InvalidBranchName(name.to_owned()));
    }

    let full_ref = format!("refs/heads/{name}");
    if !Reference::is_valid_name(&full_ref) {
        return Err(Error::InvalidBranchName(name.to_owned()));
    }

    Ok(())
}

fn validate_admin_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.contains('\0') {
        return Err(Error::InvalidAdminName(name.to_owned()));
    }
    if !name.starts_with(ADMIN_PREFIX) {
        return Err(Error::InvalidAdminName(name.to_owned()));
    }
    Ok(())
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + (value - 10)),
        _ => unreachable!(),
    }
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_feature_branch() {
        let branch = BranchName::new("feature/foo").unwrap();
        let admin = branch.encode_admin_name();

        assert!(!admin.as_str().contains('/'));
        assert_eq!(admin.decode_branch_name().unwrap(), branch);
    }

    #[test]
    fn encodes_mixed_case_and_punctuation() {
        let branch = BranchName::new("Feature/Foo-Bar.baz_123").unwrap();
        let admin = branch.encode_admin_name();

        assert_eq!(admin.decode_branch_name().unwrap(), branch);
    }

    #[test]
    fn encodes_unicode() {
        let branch = BranchName::new("føø/東京").unwrap();
        let admin = branch.encode_admin_name();

        assert_eq!(admin.decode_branch_name().unwrap(), branch);
    }

    #[test]
    fn accepts_valid_slash_and_unicode_branch_names() {
        assert!(BranchName::new("feature/foo").is_ok());
        assert!(BranchName::new("føø/東京").is_ok());
    }

    #[test]
    fn rejects_invalid_git_reference_names() {
        for invalid in ["../x", "x/../y", "foo.lock", "foo/", "foo//bar"] {
            assert_eq!(
                BranchName::new(invalid).unwrap_err().to_string(),
                Error::InvalidBranchName(invalid.to_owned()).to_string()
            );
        }
    }
}
