#![doc = include_str!("../../docs/conflict-reasons.md")]

mod display;
mod validate;
pub use display::summaries;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 检查依据的来源；通过 Evidence 推导，避免缓存里出现互相矛盾的来源字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// 原始文本的 Git 行级合并结果。
    Git,
    /// weave 给出的分类或明确拒绝；二者可能是前后相继的阶段。
    Weave,
    /// 本项目的检查和保守降级，包括因 weave 未返回分类而阻断。
    Analyze,
}

/// 父原因类别；与显示风格无关。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Git 原始文本或分区行级冲突。
    LineConflict,
    /// 实体严格规则或上游明确拒绝，至少保存一条可触发审核的依据。
    EntityConflict,
    /// 无法归入实体的对应原文区域被双方修改。
    NonEntityConflict,
    /// 文件双方变化，分析能力不足，保守阻断。
    AnalysisUnavailable,
    /// 双方变化且三方分区布局不同，保守阻断。
    LayoutChanged,
    /// 一侧移动候选与另一侧变化/未知状态关联。
    ModifyVsMove,
}

/// 原因适用的精确目标。文件路径由外层 Outcome/FileAnalysis 指定。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    /// 当前文件整体。
    File,
    /// 可靠分区的实体类型与原始名称，禁止只按显示名合并不同类型。
    Entity { entity_type: String, name: String },
    /// key 用于内部身份；label 由相邻 entity 生成，供人类定位。
    Gap { key: String, label: String },
    /// 无法绑定本地实体的上游结果；来源与序号防止同名误归并。
    Weave {
        name: String,
        source: WeaveSource,
        ordinal: usize,
    },
    /// 一条跨文件候选；不同目标/方向都是独立父原因。
    Move {
        side: Side,
        base: Location,
        target: Location,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaveSource {
    Classification,
    Refusal,
}

/// 明确的 ours/theirs 方向，不猜测 branch 名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Ours,
    Theirs,
}

/// entity 区域起点，包含附着注释；不是调用点位置。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub path: String,
    pub entity: String,
    pub line: usize,
}

/// 保留上游完整动作分类；重命名相关项是匹配候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Absent,
    Deleted,
    Unchanged,
    Modified,
    Renamed,
    RenameModified,
}
impl Change {
    pub fn changed(self) -> bool {
        !matches!(self, Self::Absent | Self::Unchanged)
    }
}
impl From<weave_core::v2::Action> for Change {
    fn from(value: weave_core::v2::Action) -> Self {
        use weave_core::v2::Action as A;
        match value {
            A::Added => Self::Added,
            A::Absent => Self::Absent,
            A::Deleted => Self::Deleted,
            A::Unchanged => Self::Unchanged,
            A::Edited => Self::Modified,
            A::Renamed => Self::Renamed,
            A::RenameEdited => Self::RenameModified,
        }
    }
}

/// 上游明确拒绝：全部 5 种，详见模块目录。升级依赖增加变体时必须更新映射。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WeaveRefusal {
    BothModified,
    ModifyDelete {
        modified_in: Side,
    },
    BothAdded,
    RenameRename {
        base_name: String,
        ours_name: String,
        theirs_name: String,
    },
    RenameModify {
        old_name: String,
        new_name: String,
        renamed_in: Side,
    },
}
impl From<&weave_core::conflict::ConflictKind> for WeaveRefusal {
    fn from(value: &weave_core::conflict::ConflictKind) -> Self {
        use weave_core::conflict::ConflictKind as K;
        match value {
            K::BothModified => Self::BothModified,
            K::BothAdded => Self::BothAdded,
            K::ModifyDelete { modified_in_ours } => Self::ModifyDelete {
                modified_in: if *modified_in_ours {
                    Side::Ours
                } else {
                    Side::Theirs
                },
            },
            K::RenameRename {
                base_name,
                ours_name,
                theirs_name,
            } => Self::RenameRename {
                base_name: base_name.clone(),
                ours_name: ours_name.clone(),
                theirs_name: theirs_name.clone(),
            },
            K::RenameModify {
                old_name,
                new_name,
                renamed_in_ours,
            } => Self::RenameModify {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
                renamed_in: if *renamed_in_ours {
                    Side::Ours
                } else {
                    Side::Theirs
                },
            },
        }
    }
}

/// 移动候选另一侧的事实，Unknown 不能误写成已证实的修改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Opposite {
    Unchanged,
    Modified,
    Deleted,
    Unknown,
}

/// 跨文件候选的文本依据；名称归一化复用 weave 公共函数，但不是上游分类。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoveMatch {
    Exact,
    NameNormalized {
        grammar: String,
        entity_type: String,
        old_name: String,
        new_name: String,
        old_occurrences: usize,
        new_occurrences: usize,
        comparison: RenameComparison,
    },
}

/// Exact comparison after name normalization; syntax mode ignores only gaps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenameComparison {
    Text,
    Syntax { tokens: usize },
}

/// 同一 base entity 在另一侧的候选；非递归，保留匹配依据及歧义数量。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OppositeMove {
    pub target: Location,
    pub matched_by: MoveMatch,
    pub source_count: usize,
    pub destination_count: usize,
}

/// 跨文件删除/新增候选；保留所有目标及实际匹配依据，不证明语义身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveEvidence {
    pub side: Side,
    pub base: Location,
    pub target: Location,
    pub source_count: usize,
    pub destination_count: usize,
    pub opposite: Opposite,
    pub matched_by: MoveMatch,
    pub opposite_moves: Vec<OppositeMove>,
}

impl MoveEvidence {
    pub(crate) fn opposite_side(&self) -> Side {
        match self.side {
            Side::Ours => Side::Theirs,
            Side::Theirs => Side::Ours,
        }
    }
}

/// 子依据全集；自身不另计一个冲突。只有挂在合法父原因下才影响判定。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Evidence {
    /// Git diff3/zdiff3 行级合并实际返回冲突，覆盖普通模式冲突。
    GitLineConflict,
    /// 未被上游实体原因覆盖的原文分区两侧均不等于 base，含格式与附着注释。
    RawBothChanged,
    /// 可靠原文分区中 ours == theirs != base；描述相同结果，不推断编辑过程相同。
    IdenticalEdits,
    /// weave 尚未拒绝此目标；复用双方动作执行本项目的严格策略。
    WeaveActions { ours: Change, theirs: Change },
    /// weave 明确拒绝，不能被本地或全局信息覆盖。
    WeaveRefusal { refusal: WeaveRefusal },
    /// 无法得到可靠的完整三方原文分区；具体限制见模块文档。
    PartitionUnavailable,
    /// 分区可靠，但 weave 未返回实体分类。
    WeaveUnavailable,
    /// 实体/间隙序列不同，不能安全按对应实体拼接。
    LayoutChanged,
    /// 跨文件移动匹配的确定性依据和另一侧状态。
    MoveCandidate { candidate: Box<MoveEvidence> },
}
impl Evidence {
    pub fn source(&self) -> Source {
        match self {
            Self::GitLineConflict => Source::Git,
            Self::WeaveActions { .. } | Self::WeaveRefusal { .. } => Source::Weave,
            Self::RawBothChanged
            | Self::IdenticalEdits
            | Self::PartitionUnavailable
            | Self::WeaveUnavailable
            | Self::LayoutChanged
            | Self::MoveCandidate { .. } => Source::Analyze,
        }
    }
}

/// 一个父节点和全部子依据。构造后用 normalize 合并同目标/同类别的证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reason {
    pub kind: Kind,
    pub subject: Subject,
    pub evidence: BTreeSet<Evidence>,
}
impl Reason {
    pub fn new(kind: Kind, subject: Subject, evidence: Evidence) -> Self {
        Self {
            kind,
            subject,
            evidence: [evidence].into(),
        }
    }
}

/// 仅合并同 kind+subject，保留全部独立证据，次序不影响输出。
pub fn normalize(reasons: &mut Vec<Reason>) {
    let mut grouped: BTreeMap<(Kind, Subject), BTreeSet<Evidence>> = BTreeMap::new();
    for reason in reasons.drain(..) {
        grouped
            .entry((reason.kind, reason.subject))
            .or_default()
            .extend(reason.evidence);
    }
    *reasons = grouped
        .into_iter()
        .map(|((kind, subject), evidence)| Reason {
            kind,
            subject,
            evidence,
        })
        .collect();
}
