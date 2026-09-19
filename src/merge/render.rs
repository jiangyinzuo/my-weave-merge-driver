//! 根据已完成的分析选择展示；只读原因，不执行分析或改变冲突判定。
use super::{conflict_box, Labels};
use crate::analysis::local::{LocalAnalysis, RegionMerge};
use crate::reason;

pub(super) fn render(
    analysis: &LocalAnalysis,
    texts: [&str; 3],
    labels: &Labels,
    width: usize,
) -> String {
    // 分区展示不得覆盖整文件 Git 冲突；weave 拒绝也可能跨越多个区域。
    if !analysis.line_conflict && !analysis.weave_refused {
        if let Some(regions) = &analysis.regions {
            if regions
                .iter()
                .any(|r| matches!(r, RegionMerge::Strict { .. }))
            {
                return regions
                    .iter()
                    .map(|region| match region {
                        RegionMerge::Strict { texts, reason } => conflict_box(
                            &texts[0],
                            &texts[1],
                            &texts[2],
                            labels,
                            width,
                            &reason.summary(),
                        ),
                        RegionMerge::Line(content) => content.clone(),
                    })
                    .collect();
            }
        }
    }
    if analysis.reasons.is_empty()
        || (analysis.reasons.len() == 1 && analysis.line_conflict)
        || analysis.compact_appends
    {
        return analysis.line_content.clone();
    }
    // 不拼接不同分析来源的冲突范围；三方共同上下文由 conflict_box 裁剪。
    conflict_box(
        texts[0],
        texts[1],
        texts[2],
        labels,
        width,
        &reason::summaries(&analysis.reasons),
    )
}
