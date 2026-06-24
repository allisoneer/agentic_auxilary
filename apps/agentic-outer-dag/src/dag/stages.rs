use crate::state::StageKind;

pub fn is_terminal(kind: &StageKind) -> bool {
    matches!(
        kind,
        StageKind::StoppedManualHandoff
            | StageKind::StoppedReviewSkipped
            | StageKind::StoppedTimedOut
            | StageKind::StoppedReadyForHumanReview
            | StageKind::StoppedDirtyTree
            | StageKind::StoppedRebaseConflict
            | StageKind::StoppedFailed
    )
}

pub fn is_paused(kind: &StageKind) -> bool {
    matches!(
        kind,
        StageKind::StoppedPermissionRequired | StageKind::StoppedQuestionRequired
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_terminal_states() {
        assert!(is_terminal(&StageKind::StoppedReadyForHumanReview));
        assert!(is_terminal(&StageKind::StoppedManualHandoff));
        assert!(is_terminal(&StageKind::StoppedDirtyTree));
        assert!(is_terminal(&StageKind::StoppedRebaseConflict));
        assert!(!is_terminal(&StageKind::WaitingForCoderabbit));
    }

    #[test]
    fn classifies_paused_states() {
        assert!(is_paused(&StageKind::StoppedPermissionRequired));
        assert!(is_paused(&StageKind::StoppedQuestionRequired));
        assert!(!is_paused(&StageKind::StoppedFailed));
    }
}
