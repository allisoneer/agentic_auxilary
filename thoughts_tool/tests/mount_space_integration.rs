#[cfg(test)]
mod tests {
    use thoughts_tool::MountSpace;

    #[test]
    fn test_mount_space_round_trip() {
        let cases = vec![
            ("thoughts", MountSpace::Thoughts),
            ("api-docs", MountSpace::Context("api-docs".to_string())),
            (
                "references/github/example",
                MountSpace::Reference {
                    org: "github".to_string(),
                    repo: "example".to_string(),
                },
            ),
        ];

        for (input, expected) in cases {
            let parsed = MountSpace::parse(input).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), input);
        }
    }

    #[test]
    fn test_mount_space_properties() {
        assert!(!MountSpace::Thoughts.is_read_only());
        assert!(!MountSpace::Context("test".to_string()).is_read_only());
        assert!(
            MountSpace::Reference {
                org: "test".to_string(),
                repo: "repo".to_string(),
            }
            .is_read_only()
        );
    }

    #[test]
    fn test_mount_space_parse_errors() {
        // Invalid reference format
        assert!(MountSpace::parse("references/invalid").is_err());
        assert!(MountSpace::parse("references/").is_err());
    }

    #[test]
    fn test_mount_space_display() {
        assert_eq!(MountSpace::Thoughts.to_string(), "thoughts");
        assert_eq!(MountSpace::Context("docs".to_string()).to_string(), "docs");
        assert_eq!(
            MountSpace::Reference {
                org: "anthropic".to_string(),
                repo: "claude".to_string(),
            }
            .to_string(),
            "references/anthropic/claude"
        );
    }
}
