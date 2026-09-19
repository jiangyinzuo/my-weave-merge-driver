//! 原文规则：布局变化、未覆盖区域双方修改，以及相同修改结果的说明。
use super::partition::{same_keys, Part, ThreePartitions};
use crate::reason::{Evidence, Kind, Reason, Subject};

/// 三侧身份和顺序已对齐；reason 仅保存严格原文/实体判定。
pub(super) struct Region {
    pub base: Part,
    pub ours: Part,
    pub theirs: Part,
    pub reason: Option<Reason>,
}

/// 无可靠分区时不猜测范围；布局不同且文件双方变化时保守阻断。
pub(super) fn analyze_regions(
    parts: ThreePartitions,
    both_changed: bool,
    reasons: &mut Vec<Reason>,
) -> Option<Vec<Region>> {
    let (Some(base), Some(ours), Some(theirs)) = parts else {
        return None;
    };
    if !same_keys(&base, &ours) || !same_keys(&base, &theirs) {
        if both_changed {
            reasons.push(Reason::new(
                Kind::LayoutChanged,
                Subject::File,
                Evidence::LayoutChanged,
            ));
        }
        return None;
    }
    Some(
        base.into_iter()
            .zip(ours)
            .zip(theirs)
            .map(|((base, ours), theirs)| {
                let reason = record_part_conflict(&base, &ours, &theirs, reasons);
                Region {
                    base,
                    ours,
                    theirs,
                    reason,
                }
            })
            .collect(),
    )
}

/// Reuse an entity decision, or record raw changes in an uncovered region.
/// weave normalizes entity text and does not classify interstitial text.
fn record_part_conflict(
    base: &Part,
    ours: &Part,
    theirs: &Part,
    reasons: &mut Vec<Reason>,
) -> Option<Reason> {
    let subject = base.subject();
    if let Some(reason) = reasons
        .iter_mut()
        .find(|r| r.kind == Kind::EntityConflict && r.subject == subject)
    {
        if base.text != ours.text && ours.text == theirs.text {
            reason.evidence.insert(Evidence::IdenticalEdits);
        }
        return Some(reason.clone());
    }
    if base.text == ours.text || base.text == theirs.text {
        return None;
    }
    let reason = Reason::new(
        if base.entity {
            Kind::EntityConflict
        } else {
            Kind::NonEntityConflict
        },
        subject,
        if base.entity && ours.text == theirs.text {
            Evidence::IdenticalEdits
        } else {
            Evidence::RawBothChanged
        },
    );
    reasons.push(reason.clone());
    Some(reason)
}
