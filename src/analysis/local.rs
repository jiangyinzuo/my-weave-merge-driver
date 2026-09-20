//! 单文件分析顺序：Git 基线 → weave 拒绝 → 严格分类 → 原文规则 → 分区行级检查。
//! 返回全部原因及可用的展示材料；调用方只能选择展示，不能撤销原因。
use super::{
    line::line_merge,
    partition::{distinct_entity_appends, partition, ThreePartitions},
    raw::{analyze_regions, Region},
    upstream::{add_strict_entity_reasons, weave_refusals},
};
use crate::merge::{ConflictStyle, Labels};
use crate::reason::{self, Evidence, Kind, Reason, Subject};
use anyhow::Result;

pub(crate) enum RegionMerge {
    Strict { index: usize, reason: Reason },
    Line(String),
}

/// 分析后的只读展示材料。全文件与分区的 Git 输出均已完成计算。
pub(crate) struct LocalAnalysis {
    pub reasons: Vec<Reason>,
    pub partitions: ThreePartitions,
    pub line_content: String,
    pub line_conflict: bool,
    pub weave_refused: bool,
    pub regions: Option<Vec<RegionMerge>>,
    /// 仅允许紧凑展示，不取消布局或行级冲突。
    pub compact_appends: bool,
}

/// 单文件与全局入口共用输入校验及完整规则；仅最终展示由调用方选择。
pub(crate) fn analyze(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    labels: &Labels,
    width: usize,
    style: ConflictStyle,
) -> Result<LocalAnalysis> {
    crate::merge::validate_inputs([base, ours, theirs], width)?;
    // 1. 原始整文件 Git 冲突是独立下限，不能被后续分析消除。
    let (line_content, line_conflict) = line_merge(base, ours, theirs, labels, width, style)?;
    let upstream = weave_core::entity_merge(base, ours, theirs, path);
    let parts = (
        partition(path, base),
        partition(path, ours),
        partition(path, theirs),
    );
    let partitions_reliable = matches!(&parts, (Some(_), Some(_), Some(_)));

    // 2. 优先采用 weave 拒绝；仅为未覆盖的可靠目标补充分类上的严格规则。
    let mut reasons = weave_refusals(&upstream.conflicts, &parts);
    if line_conflict {
        reasons.push(Reason::new(
            Kind::LineConflict,
            Subject::File,
            Evidence::GitLineConflict,
        ));
    }
    add_strict_entity_reasons(base, ours, theirs, path, &parts, &mut reasons);

    // 3. 检查布局与未覆盖原文；为已有原因补充确定性的相同结果说明。
    let regions = analyze_regions(&parts, ours != base && theirs != base, &mut reasons);

    // 4. 没有严格原因的区域由 Git 合并；其冲突也必须在分析阶段记录。
    let regions = regions
        .map(|regions| analyze_region_lines(regions, labels, width, style, &mut reasons))
        .transpose()?;

    // 只证明这一种布局变化可采用 Git 的紧凑输出；所有阻断原因保留。
    let compact_appends = style == ConflictStyle::Zdiff3
        && line_conflict
        && partitions_reliable
        && reasons
            .iter()
            .all(|r| matches!(r.kind, Kind::LineConflict | Kind::LayoutChanged))
        && distinct_entity_appends(base, ours, theirs, path);

    reason::normalize(&mut reasons);
    Ok(LocalAnalysis {
        reasons,
        partitions: parts,
        line_content,
        line_conflict,
        weave_refused: !upstream.conflicts.is_empty(),
        regions,
        compact_appends,
    })
}

/// 已有严格原因的区域保留三方原文，其余区域记录 Git 输出及行级原因。
fn analyze_region_lines(
    regions: Vec<Region<'_>>,
    labels: &Labels,
    width: usize,
    style: ConflictStyle,
    reasons: &mut Vec<Reason>,
) -> Result<Vec<RegionMerge>> {
    let mut result = Vec::with_capacity(regions.len());
    for (index, region) in regions.into_iter().enumerate() {
        if let Some(reason) = region.reason {
            result.push(RegionMerge::Strict { index, reason });
            continue;
        }
        let (content, conflict) = line_merge(
            &region.base.text,
            &region.ours.text,
            &region.theirs.text,
            labels,
            width,
            style,
        )?;
        if conflict {
            reasons.push(Reason::new(
                Kind::LineConflict,
                region.base.subject(),
                Evidence::GitLineConflict,
            ));
        }
        result.push(RegionMerge::Line(content));
    }
    Ok(result)
}

impl LocalAnalysis {
    /// Strict region indices are produced only after all three layouts align.
    pub fn region_texts(&self, index: usize) -> [&str; 3] {
        [&self.partitions.0, &self.partitions.1, &self.partitions.2].map(|parts| {
            parts.as_ref().expect("aligned partitions")[index]
                .text
                .as_str()
        })
    }
}
