#![doc = include_str!("../docs/conflict-reasons.md")]

use crate::merge::safe_label;
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

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Weave => "weave",
            Self::Analyze => "analyze",
        }
    }
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
impl Kind {
    pub fn code(self) -> &'static str {
        match self {
            Self::LineConflict => "LINE_CONFLICT",
            Self::EntityConflict => "ENTITY_CONFLICT",
            Self::NonEntityConflict => "UNMODELED_BOTH_CHANGED",
            Self::AnalysisUnavailable => "ENTITY_ANALYSIS_UNAVAILABLE",
            Self::LayoutChanged => "ENTITY_LAYOUT_CHANGED",
            Self::ModifyVsMove => "GLOBAL_MODIFY_VS_MOVE",
        }
    }
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
impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ours => "ours",
            Self::Theirs => "theirs",
        }
    }
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Absent => "absent",
            Self::Deleted => "deleted",
            Self::Unchanged => "unchanged",
            Self::Modified => "modified",
            Self::Renamed => "rename candidate",
            Self::RenameModified => "rename + modified candidate",
        }
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
impl WeaveRefusal {
    /// 展示上游拒绝类型，并保留方向和重命名名称。
    pub fn summary(&self) -> String {
        match self {
            Self::BothModified => "both_modified".into(),
            Self::BothAdded => "both_added".into(),
            Self::ModifyDelete { modified_in } => {
                format!("modify_delete；modified in {}", modified_in.label())
            }
            Self::RenameRename {
                base_name,
                ours_name,
                theirs_name,
            } => format!("rename_rename；base={base_name}, ours={ours_name}, theirs={theirs_name}"),
            Self::RenameModify {
                old_name,
                new_name,
                renamed_in,
            } => format!(
                "rename_modify；{}: {old_name} → {new_name}",
                renamed_in.label()
            ),
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
                    !self.base.entity.is_empty() && self.base.entity == self.target.entity
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
                        && self.base.entity == format!("{entity_type} {old_name}")
                        && self.target.entity == format!("{entity_type} {new_name}")
                }
            }
    }

    pub(crate) fn opposite_side(&self) -> Side {
        match self.side {
            Side::Ours => Side::Theirs,
            Side::Theirs => Side::Ours,
        }
    }

    /// 文件级关联提示，不声称候选一定属于紧邻的某个行级冲突块。
    pub(crate) fn marker_note(&self) -> String {
        move_note(&self.base, &self.target, &self.matched_by)
    }

    pub(crate) fn opposite_marker_notes(&self) -> impl Iterator<Item = String> + '_ {
        self.opposite_moves
            .iter()
            .map(|other| move_note(&self.base, &other.target, &other.matched_by))
    }
}

fn move_note(base: &Location, target: &Location, matched_by: &MoveMatch) -> String {
    let action = match matched_by {
        MoveMatch::Exact => "疑似移动",
        MoveMatch::NameNormalized { .. } => "疑似重命名并移动",
    };
    safe_label(&format!(
        "{action} {} · {}:{} → {} · {}:{}",
        base.entity, base.path, base.line, target.entity, target.path, target.line
    ))
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

    pub fn summary(&self) -> String {
        let message = match self {
            Self::GitLineConflict => "Git 行级合并返回冲突".into(),
            Self::RawBothChanged => "原文：ours、theirs 均相对 base 改变".into(),
            Self::IdenticalEdits => "原文：ours 与 theirs 逐 byte 相同，且均不同于 base".into(),
            Self::WeaveActions { ours, theirs } => {
                format!("分类：ours={}, theirs={}", ours.label(), theirs.label())
            }
            Self::WeaveRefusal { refusal } => format!("拒绝：{}", refusal.summary()),
            Self::PartitionUnavailable => {
                "原文分区不可用：语言、语法、实体边界或唯一性检查未通过".into()
            }
            Self::WeaveUnavailable => "weave 未返回实体分类".into(),
            Self::LayoutChanged => "三方实体或非实体区域序列不同".into(),
            Self::MoveCandidate { candidate: c } => {
                let mut opposite = match c.opposite {
                    Opposite::Unchanged => "unchanged",
                    Opposite::Modified => "modified",
                    Opposite::Deleted => "deleted",
                    Opposite::Unknown => "无法确认",
                }
                .to_owned();
                if !c.opposite_moves.is_empty() {
                    let targets = c.opposite_marker_notes().collect::<Vec<_>>().join("；");
                    opposite.push_str(&format!(
                        "（{}：{}；候选数={}）",
                        c.opposite_side().label(),
                        targets,
                        c.opposite_moves.len()
                    ));
                }
                if let MoveMatch::NameNormalized {
                    grammar,
                    entity_type,
                    old_name,
                    new_name,
                    old_occurrences,
                    new_occurrences,
                    comparison,
                } = &c.matched_by
                {
                    let matched = match comparison {
                        RenameComparison::Text => "区域文本逐 byte 相同（含附着注释）".into(),
                        RenameComparison::Syntax { tokens } => format!(
                            "忽略 token 间空白后语法结构及有序 token 完全相同（{tokens} 个 token，保留注释和字面量原文）；原文存在格式差异"
                        ),
                    };
                    return safe_label(&format!(
                        "[analyze]：RENAME_MOVE_CANDIDATE：{} 疑似重命名并移动 {entity_type} {old_name} → {new_name} · {}:{} → {}:{}；源 deleted、目标 added；grammar={grammar}、类型相同；按词边界替换各自名称后，{matched}；替换次数={old_occurrences}/{new_occurrences}；sources={}，destinations={}；另一侧={opposite}；仅为文本候选，未证明语义等价",
                        c.side.label(), c.base.path, c.base.line, c.target.path, c.target.line, c.source_count, c.destination_count
                    ));
                }
                format!(
                    "MOVE_CANDIDATE：{} 疑似移动 {} · {}:{} → {}:{}；\
                     源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；\
                     sources={}，destinations={}；另一侧={opposite}",
                    c.side.label(),
                    c.base.entity,
                    c.base.path,
                    c.base.line,
                    c.target.path,
                    c.target.line,
                    c.source_count,
                    c.destination_count
                )
            }
        };
        safe_label(&format!("[{}]：{message}", self.source().label()))
    }

    /// 详细模式补充方法与适用边界；摘要已包含候选方向、位置、匹配条件和歧义数量。
    pub fn lines(&self, detailed: bool) -> Vec<String> {
        let mut lines = vec![self.summary()];
        if detailed {
            if let Self::MoveCandidate { candidate } = self {
                for other in &candidate.opposite_moves {
                    let counterpart = MoveEvidence {
                        side: candidate.opposite_side(),
                        base: candidate.base.clone(),
                        target: other.target.clone(),
                        source_count: other.source_count,
                        destination_count: other.destination_count,
                        opposite: Opposite::Deleted,
                        matched_by: other.matched_by.clone(),
                        opposite_moves: Vec::new(),
                    };
                    lines.push(format!(
                        "另一侧候选 {}",
                        Self::MoveCandidate {
                            candidate: Box::new(counterpart)
                        }
                        .summary()
                    ));
                }
            }
        }
        if detailed
            && matches!(self, Self::MoveCandidate { candidate }
            if matches!(candidate.matched_by, MoveMatch::NameNormalized { .. }))
        {
            if matches!(self, Self::MoveCandidate { candidate }
            if matches!(candidate.matched_by, MoveMatch::NameNormalized {
                comparison: RenameComparison::Syntax { .. }, ..
            })) {
                lines.push("[analyze]：方法：复用 weave::binding::replace_at_word_boundaries 和 sem-core::parse_tree，比较名称归一化后的完整语法结构及 token 序列；相同才匹配，不使用模糊阈值或 hash 判等".into());
                lines.push("[analyze]：格式：只忽略语法 token 间的空白；保留节点类型、字段、顺序、嵌套及注释/字面量原文，不把缩进造成的结构变化当作格式化；不是语义等价证明".into());
            } else {
                lines.push("[analyze]：方法：复用 weave::binding::replace_at_word_boundaries，将各自名称替换为 __ENTITY__ 后直接比较原文；不使用相似度或 hash 判等".into());
            }
            lines.push("[analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新".into());
            lines.push("[analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对".into());
        }
        lines
    }

    fn necessary(&self) -> bool {
        !matches!(
            self,
            Self::GitLineConflict
                | Self::RawBothChanged
                | Self::IdenticalEdits
                | Self::LayoutChanged
        )
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
    pub fn summary(&self) -> String {
        let target = match &self.subject {
            Subject::File => String::new(),
            Subject::Entity { entity_type, name } => format!("{entity_type} {name}"),
            Subject::Gap { label, .. } => label.clone(),
            Subject::Weave { name, .. } => format!("entity {name}（weave，身份未关联）"),
            Subject::Move { base, .. } => base.entity.clone(),
        };
        safe_label(&format!(
            "{}：{}",
            self.kind.code(),
            match self.kind {
                Kind::LineConflict =>
                    if target.is_empty() {
                        "Git 行级冲突".into()
                    } else {
                        format!("Git 行级冲突 · {target}")
                    },
                Kind::EntityConflict =>
                    if self.evidence.contains(&Evidence::IdenticalEdits) {
                        format!("{target}：双方修改结果相同，但均不同于 base，按严格规则需人工审核")
                    } else {
                        format!("{target} 需人工审核")
                    },
                Kind::NonEntityConflict => format!("{target}被双方修改"),
                Kind::AnalysisUnavailable => "无法可靠分析，保留整文件冲突".into(),
                Kind::LayoutChanged => "实体增删或顺序变化".into(),
                Kind::ModifyVsMove => {
                    if let Some((old_name, new_name)) = self.evidence.iter().find_map(|e| match e {
                        Evidence::MoveCandidate { candidate } => match &candidate.matched_by {
                            MoveMatch::NameNormalized {
                                old_name, new_name, ..
                            } => Some((old_name, new_name)),
                            _ => None,
                        },
                        _ => None,
                    }) {
                        format!("{target} 疑似重命名并移动（{old_name} → {new_name}），与另一侧变化需共同审核")
                    } else {
                        format!("{target} 疑似移动与另一侧变化需共同审核")
                    }
                }
            }
        ))
    }
    /// 摘要/详细输出共用同一父子数据；折叠不删除 evidence。
    pub fn lines(&self, detailed: bool) -> Vec<String> {
        let sources: BTreeSet<_> = self.evidence.iter().map(Evidence::source).collect();
        let sources = sources
            .into_iter()
            .map(Source::label)
            .collect::<Vec<_>>()
            .join(" + ");
        let mut lines = vec![format!("原因 [{sources}]：{}", self.summary())];
        for evidence in &self.evidence {
            if detailed || evidence.necessary() {
                lines.extend(
                    evidence
                        .lines(detailed)
                        .into_iter()
                        .map(|line| format!("  依据 {line}")),
                );
            }
        }
        lines
    }
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

/// marker 中只放父原因摘要，完整子证据保存在报告/详细诊断中。
pub fn summaries(reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(Reason::summary)
        .collect::<Vec<_>>()
        .join("; ")
}
