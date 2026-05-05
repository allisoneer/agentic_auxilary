#[derive(Debug)]
pub enum OrchestratorError {
    ExternalServerUnavailable { base_url: String, reason: String },
}

impl std::fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExternalServerUnavailable { base_url, reason } => {
                write!(
                    f,
                    "External OpenCode server unavailable (base_url={base_url}): {reason}"
                )
            }
        }
    }
}

impl std::error::Error for OrchestratorError {}
