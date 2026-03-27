use anyhow::Result;
use anyhow::bail;
use sha2::Digest;
use sha2::Sha256;

const MAX_REF_KEY_LEN: usize = 120;
const HASH_HEX_LEN: usize = 16;
const PREFIX: &str = "r-";

pub fn encode_ref_key(ref_name: &str) -> Result<String> {
    let ref_name = ref_name.trim();
    if ref_name.is_empty() {
        bail!("Reference name cannot be empty");
    }
    if ref_name.contains('\0') {
        bail!("Reference name cannot contain NUL bytes");
    }
    if looks_like_raw_git_oid(ref_name) {
        bail!("Raw commit SHAs are not supported; provide a named ref instead");
    }

    let mut encoded = String::with_capacity(ref_name.len() + PREFIX.len());
    encoded.push_str(PREFIX);
    for byte in ref_name.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => encoded.push(byte as char),
            _ => push_escape(&mut encoded, byte),
        }
    }

    if encoded.len() <= MAX_REF_KEY_LEN {
        return Ok(encoded);
    }

    let hash = short_hash_hex(ref_name.as_bytes());
    let suffix = format!("--{hash}");
    let keep = MAX_REF_KEY_LEN
        .checked_sub(PREFIX.len() + suffix.len())
        .ok_or_else(|| anyhow::anyhow!("Invalid ref key length configuration"))?;

    let mut truncated = String::with_capacity(MAX_REF_KEY_LEN);
    truncated.push_str(PREFIX);
    truncated.push_str(&truncate_on_char_boundary(&encoded[PREFIX.len()..], keep));
    truncated.push_str(&suffix);
    Ok(truncated)
}

fn looks_like_raw_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn push_escape(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('~');
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}

fn short_hash_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..(HASH_HEX_LEN / 2)]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn truncate_on_char_boundary(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut end = max_len;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::MAX_REF_KEY_LEN;
    use super::encode_ref_key;

    #[test]
    fn encodes_safe_ascii_directly() {
        assert_eq!(encode_ref_key("main").unwrap(), "r-main");
        assert_eq!(encode_ref_key("release-1.2_3").unwrap(), "r-release-1.2_3");
    }

    #[test]
    fn escapes_slashes_and_uppercase_for_case_safe_segments() {
        assert_eq!(encode_ref_key("feature/foo").unwrap(), "r-feature~2ffoo");
        assert_eq!(encode_ref_key("Main").unwrap(), "r-~4dain");
    }

    #[test]
    fn rejects_empty_nul_and_raw_sha_values() {
        assert!(encode_ref_key("   ").is_err());
        assert!(encode_ref_key("abc\0def").is_err());
        assert!(encode_ref_key("0123456789abcdef0123456789abcdef01234567").is_err());
    }

    #[test]
    fn truncates_long_values_with_stable_structure() {
        let input = "refs/heads/".to_string() + &"very-long-".repeat(40);
        let out = encode_ref_key(&input).unwrap();

        assert!(out.starts_with("r-"), "must retain r- prefix");
        assert!(
            out.len() <= MAX_REF_KEY_LEN,
            "must be bounded by MAX_REF_KEY_LEN"
        );

        let (_prefix_part, hash) = out.rsplit_once("--").expect("expected --{hash} suffix");
        assert_eq!(hash.len(), 16, "expected 16 hex chars");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn truncation_hash_makes_long_keys_unique() {
        let base = "refs/heads/".to_string() + &"a".repeat(300);
        let a = format!("{}-x", base);
        let b = format!("{}-y", base);

        let ka = encode_ref_key(&a).unwrap();
        let kb = encode_ref_key(&b).unwrap();
        assert_ne!(ka, kb, "distinct long refs must not collide");
    }
}
