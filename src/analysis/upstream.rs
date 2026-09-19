//! weave 拒绝与分类：保留上游拒绝，针对未覆盖目标补充双方变更严格策略。
//! 不重写 weave 的匹配、分类或拒绝算法。
use super::partition::{local_subject, ThreePartitions};
use crate::reason::{Change, Evidence, Kind, Reason, Subject, WeaveSource};
use std::collections::BTreeMap;
use weave_core::v2::analyze_default;

/// Collect weave's refusals before applying any additional strict policy.
pub(super) fn weave_refusals(
    conflicts: &[weave_core::conflict::EntityConflict],
    parts: &ThreePartitions,
) -> Vec<Reason> {
    conflicts
        .iter()
        .enumerate()
        .map(|(ordinal, conflict)| {
            let subject = local_subject(parts, &conflict.entity_name, Some(&conflict.entity_type))
                .unwrap_or_else(|| Subject::Weave {
                    name: conflict.entity_name.clone(),
                    source: WeaveSource::Refusal,
                    ordinal,
                });
            Reason::new(
                Kind::EntityConflict,
                subject,
                Evidence::WeaveRefusal {
                    refusal: (&conflict.kind).into(),
                },
            )
        })
        .collect()
}

/// Supplement weave's refusals with the strict both-changed policy. A
/// byte-identical final pair is not a clean fast path: both may differ from base.
pub(super) fn add_strict_entity_reasons(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    parts: &ThreePartitions,
    reasons: &mut Vec<Reason>,
) {
    if ours == base || theirs == base {
        return;
    }
    if !matches!(parts, (Some(_), Some(_), Some(_))) {
        reasons.push(Reason::new(
            Kind::AnalysisUnavailable,
            Subject::File,
            Evidence::PartitionUnavailable,
        ));
        return;
    }
    let Some(analysis) = analyze_default(base, ours, theirs, path) else {
        reasons.push(Reason::new(
            Kind::AnalysisUnavailable,
            Subject::File,
            Evidence::WeaveUnavailable,
        ));
        return;
    };
    let mut counts = BTreeMap::new();
    for (triple, _) in analysis.iter() {
        *counts.entry(analysis.label(triple)).or_insert(0) += 1;
    }
    for (ordinal, (triple, cell)) in analysis.iter().enumerate() {
        let (o, t) = cell.actions();
        let (ours, theirs) = (Change::from(o), Change::from(t));
        if !ours.changed() || !theirs.changed() {
            continue;
        }
        let name = analysis.label(triple);
        let subject = if counts[&name] == 1 {
            local_subject(parts, &name, None)
        } else {
            None
        }
        .unwrap_or(Subject::Weave {
            name,
            source: WeaveSource::Classification,
            ordinal,
        });
        // weave owns refusal rules. Add our policy only for entities not
        // already covered by a refusal with the same reliable identity.
        if reasons
            .iter()
            .any(|r| r.kind == Kind::EntityConflict && r.subject == subject)
        {
            continue;
        }
        reasons.push(Reason::new(
            Kind::EntityConflict,
            subject,
            Evidence::WeaveActions { ours, theirs },
        ));
    }
}
