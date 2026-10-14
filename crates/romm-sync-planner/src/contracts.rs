use crate::{ActivityStatus, AdapterSnapshot, GameKey, IdentityStatus, LocalSaveState, RootStatus};

/// Discovery evidence passed to an adapter without granting it filesystem access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterObservation {
    pub key: GameKey,
    pub identity: IdentityStatus,
    pub root: RootStatus,
    pub local: LocalSaveState,
}

/// A stable adapter-normalization failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterIssue {
    AmbiguousIdentity,
    MissingIdentity,
    MissingRoot,
    UnmountedRoot,
    IncompleteGroup,
    MalformedGroup,
    UnsupportedFormat,
}

/// Pure adapter boundary used to turn discovery evidence into planner facts.
///
/// Implementations in M1A must be deterministic and may inspect only the
/// supplied value. Later discovery code may perform I/O before invoking this
/// contract, but that I/O is outside the planner.
pub trait AdapterContract {
    fn descriptor(&self) -> AdapterSnapshot;

    /// Normalize already-collected discovery evidence.
    ///
    /// # Errors
    ///
    /// Returns an [`AdapterIssue`] when the supplied evidence cannot safely be
    /// represented as a complete, compatible save group.
    fn normalize(
        &self,
        observation: &AdapterObservation,
    ) -> Result<AdapterObservation, AdapterIssue>;
}

/// Evidence collected by an activity provider at an explicit observation revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityEvidence {
    pub detector_id: String,
    pub observation_revision: String,
    pub active_signals: Vec<String>,
    pub inactive_signals: Vec<String>,
    pub collection_complete: bool,
}

/// Pure activity-classification boundary.
///
/// OS-specific process, session, and launcher probes are deliberately excluded.
/// They will produce `ActivityEvidence`; this interface only classifies it.
pub trait ActivityDetectionContract {
    fn classify(&self, evidence: &ActivityEvidence) -> ActivityStatus;
}

/// Conservative reference classifier used by adapters and tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConservativeActivityClassifier;

impl ActivityDetectionContract for ConservativeActivityClassifier {
    fn classify(&self, evidence: &ActivityEvidence) -> ActivityStatus {
        if !evidence.active_signals.is_empty() {
            ActivityStatus::Active
        } else if evidence.collection_complete && !evidence.inactive_signals.is_empty() {
            ActivityStatus::Inactive
        } else {
            ActivityStatus::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_classification_is_conservative() {
        let classifier = ConservativeActivityClassifier;
        let incomplete = ActivityEvidence {
            detector_id: "test".to_owned(),
            observation_revision: "1".to_owned(),
            active_signals: Vec::new(),
            inactive_signals: vec!["process-not-found".to_owned()],
            collection_complete: false,
        };
        assert_eq!(classifier.classify(&incomplete), ActivityStatus::Unknown);

        let active = ActivityEvidence {
            active_signals: vec!["retroarch-running".to_owned()],
            ..incomplete
        };
        assert_eq!(classifier.classify(&active), ActivityStatus::Active);
    }
}
