//! Validate deserialized evidence without changing or repairing it.
use super::*;

impl MoveEvidence {
    /// 纯移动也可以是合法关联，但不是阻断原因。
    pub fn valid(&self) -> bool {
        self.valid_match()
            && (self.opposite_moves.is_empty() || self.opposite == Opposite::Deleted)
            && self.opposite_moves.windows(2).all(|pair| pair[0] < pair[1])
            && self.opposite_moves.iter().all(|other| {
                Self {
                    side: self.side,
                    base: self.base.clone(),
                    target: other.target.clone(),
                    source_count: other.source_count,
                    destination_count: other.destination_count,
                    opposite: Opposite::Deleted,
                    matched_by: other.matched_by.clone(),
                    opposite_moves: Vec::new(),
                }
                .valid_match()
            })
    }

    fn valid_match(&self) -> bool {
        self.source_count > 0
            && self.destination_count > 0
            && !self.base.path.is_empty()
            && !self.target.path.is_empty()
            && self.base.path != self.target.path
            && self.base.line > 0
            && self.target.line > 0
            && match &self.matched_by {
                MoveMatch::Exact => {
                    !self.base.entity_type.is_empty()
                        && !self.base.name.is_empty()
                        && self.base.entity_type == self.target.entity_type
                        && self.base.name == self.target.name
                }
                MoveMatch::NameNormalized {
                    grammar,
                    entity_type,
                    old_name,
                    new_name,
                    old_occurrences,
                    new_occurrences,
                    comparison,
                } => {
                    !grammar.is_empty()
                        && !entity_type.is_empty()
                        && !old_name.is_empty()
                        && !new_name.is_empty()
                        && old_name != new_name
                        && *old_occurrences > 0
                        && old_occurrences == new_occurrences
                        && match comparison {
                            RenameComparison::Text => true,
                            RenameComparison::Syntax { tokens } => *tokens > 0,
                        }
                        && self.base.entity_type == *entity_type
                        && self.target.entity_type == *entity_type
                        && self.base.name == *old_name
                        && self.target.name == *new_name
                }
            }
    }
}

impl Reason {
    /// 检查反序列化数据的父子组合，拒绝空证据及矛盾类型。
    pub fn valid(&self) -> bool {
        let subject_ok = match self.kind {
            Kind::LineConflict => matches!(
                self.subject,
                Subject::File | Subject::Entity { .. } | Subject::Gap { .. }
            ),
            Kind::EntityConflict => {
                matches!(self.subject, Subject::Entity { .. } | Subject::Weave { .. })
            }
            Kind::NonEntityConflict => matches!(self.subject, Subject::Gap { .. }),
            Kind::AnalysisUnavailable | Kind::LayoutChanged => {
                matches!(self.subject, Subject::File)
            }
            Kind::ModifyVsMove => matches!(self.subject, Subject::Move { .. }),
        };
        subject_ok
            && match &self.subject {
                Subject::Gap { key, label } => !key.is_empty() && !label.trim().is_empty(),
                _ => true,
            }
            && !self.evidence.is_empty()
            && self.evidence.iter().all(|e| match (self.kind, e) {
                (Kind::LineConflict, Evidence::GitLineConflict)
                | (Kind::EntityConflict | Kind::NonEntityConflict, Evidence::RawBothChanged)
                | (Kind::EntityConflict, Evidence::WeaveRefusal { .. })
                | (
                    Kind::AnalysisUnavailable,
                    Evidence::PartitionUnavailable | Evidence::WeaveUnavailable,
                )
                | (Kind::LayoutChanged, Evidence::LayoutChanged) => true,
                (Kind::EntityConflict, Evidence::IdenticalEdits) => {
                    matches!(self.subject, Subject::Entity { .. })
                }
                (Kind::EntityConflict, Evidence::WeaveActions { ours, theirs }) => {
                    ours.changed() && theirs.changed()
                }
                (Kind::ModifyVsMove, Evidence::MoveCandidate { candidate: c }) => {
                    c.opposite != Opposite::Unchanged
                        && c.valid()
                        && self.subject
                            == Subject::Move {
                                side: c.side,
                                base: c.base.clone(),
                                target: c.target.clone(),
                            }
                }
                _ => false,
            })
    }
}
