//! Human-readable summaries and marker notes; no conflict decisions.
use super::*;
use crate::merge::safe_label;

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Weave => "weave",
            Self::Analyze => "analyze",
        }
    }
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

impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ours => "ours",
            Self::Theirs => "theirs",
        }
    }
}

impl Change {
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

impl MoveEvidence {
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

impl Evidence {
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

impl Reason {
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

/// marker 中只放父原因摘要，完整子证据保存在报告/详细诊断中。
pub fn summaries(reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(Reason::summary)
        .collect::<Vec<_>>()
        .join("; ")
}
