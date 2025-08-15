/// Sanitize a mount name for use as directory name
pub fn sanitize_mount_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_mount_name() {
        assert_eq!(sanitize_mount_name("valid-name_123"), "valid-name_123");
        assert_eq!(sanitize_mount_name("bad name!@#"), "bad_name___");
        assert_eq!(sanitize_mount_name("CamelCase"), "CamelCase");
    }
}
