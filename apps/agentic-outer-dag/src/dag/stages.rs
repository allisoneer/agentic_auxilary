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

pub fn sequence_index(kind: &StageKind) -> Option<u8> {
    match kind {
        StageKind::Init => Some(0),
        StageKind::FreshnessBeforeTicketToPr => Some(1),
        StageKind::DispatchingTicketToPr => Some(2),
        StageKind::DetectingPr => Some(3),
        StageKind::FreshnessBeforeCoderabbitWait => Some(4),
        StageKind::WaitingForCoderabbit => Some(5),
        StageKind::DispatchingResolvePrComments => Some(6),
        _ => None,
    }
}

pub fn is_beyond_stop_after(current: &StageKind, stop_after: &StageKind) -> bool {
    match (sequence_index(current), sequence_index(stop_after)) {
        (Some(current), Some(stop_after)) => current > stop_after,
        _ => false,
    }
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

    #[test]
    fn sequence_index_orders_active_pipeline_stages() {
        assert_eq!(sequence_index(&StageKind::Init), Some(0));
        assert_eq!(
            sequence_index(&StageKind::FreshnessBeforeTicketToPr),
            Some(1)
        );
        assert_eq!(sequence_index(&StageKind::DispatchingTicketToPr), Some(2));
        assert_eq!(sequence_index(&StageKind::DetectingPr), Some(3));
        assert_eq!(
            sequence_index(&StageKind::FreshnessBeforeCoderabbitWait),
            Some(4)
        );
        assert_eq!(sequence_index(&StageKind::WaitingForCoderabbit), Some(5));
        assert_eq!(
            sequence_index(&StageKind::DispatchingResolvePrComments),
            Some(6)
        );
        assert_eq!(sequence_index(&StageKind::StoppedFailed), None);
    }

    #[test]
    fn identifies_when_current_stage_is_beyond_stop_after_ceiling() {
        assert!(is_beyond_stop_after(
            &StageKind::DispatchingResolvePrComments,
            &StageKind::WaitingForCoderabbit,
        ));
        assert!(!is_beyond_stop_after(
            &StageKind::WaitingForCoderabbit,
            &StageKind::WaitingForCoderabbit,
        ));
        assert!(!is_beyond_stop_after(
            &StageKind::DetectingPr,
            &StageKind::WaitingForCoderabbit,
        ));
        assert!(!is_beyond_stop_after(
            &StageKind::StoppedFailed,
            &StageKind::WaitingForCoderabbit,
        ));
    }
}
