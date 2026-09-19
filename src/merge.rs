use anyhow::{bail, Context, Result};
use sem_core::parser::plugins::create_default_registry;
use std::{collections::BTreeSet, io::Write, process::Command};
use weave_core::{
    region::{extract_regions, FileRegion},
    v2::{analyze_default, Action},
};

pub const MAX_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConflictStyle {
    #[default]
    Diff3,
    Zdiff3,
}

#[derive(Clone, Debug)]
pub struct Labels {
    pub ours: String,
    pub base: String,
    pub theirs: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            ours: "来源未识别".into(),
            base: "来源未识别".into(),
            theirs: "来源未识别".into(),
        }
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub content: String,
    pub reasons: Vec<String>,
}

impl Outcome {
    pub fn conflicted(&self) -> bool {
        !self.reasons.is_empty()
    }
}

// Prevent multiline labels from becoming source text or terminal control sequences.
pub fn safe_label(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn validate_text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > MAX_BYTES {
        bail!("初版仅支持不超过 {MAX_BYTES} bytes 的文本文件");
    }
    if bytes.contains(&0) {
        bail!("初版不支持 binary 文件");
    }
    std::str::from_utf8(bytes).context("初版仅支持 UTF-8 文本")
}

fn validate_inputs(texts: [&str; 3], width: usize) -> Result<()> {
    if !(7..=100).contains(&width) {
        bail!("marker size 必须在 7..=100 之间");
    }
    for text in texts {
        validate_text(text.as_bytes())?;
        // Refuse rather than nest an already conflicted input. This deliberately
        // also rejects source fixtures quoting marker lines.
        if text.lines().any(|line| {
            ["<<<<<<<", "|||||||", "=======", ">>>>>>>"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        }) {
            bail!("输入已有 conflict marker；保持输入不变，需先处理现有冲突");
        }
    }
    Ok(())
}

pub fn conflict_box(
    base: &str,
    ours: &str,
    theirs: &str,
    labels: &Labels,
    width: usize,
    reason: &str,
) -> String {
    let section = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            s.to_owned()
        } else {
            format!("{s}\n")
        }
    };
    format!(
        "{} ours: {} | {}\n{}{} base: {}\n{}{}\n{}{} theirs: {}\n",
        "<".repeat(width),
        safe_label(&labels.ours),
        safe_label(reason),
        section(ours),
        "|".repeat(width),
        safe_label(&labels.base),
        section(base),
        "=".repeat(width),
        section(theirs),
        ">".repeat(width),
        safe_label(&labels.theirs)
    )
}

/// The independent, original-text line baseline. Never use weave's fallback as
/// the baseline, and never mistake a process error for a clean merge.
fn line_merge(
    base: &str,
    ours: &str,
    theirs: &str,
    labels: &Labels,
    width: usize,
    style: ConflictStyle,
) -> Result<(String, bool)> {
    let mut files = Vec::new();
    for text in [ours, base, theirs] {
        let mut f = tempfile::NamedTempFile::new()?;
        f.write_all(text.as_bytes())?;
        files.push(f);
    }
    let run = |diff3: bool| -> Result<(String, bool)> {
        let mut command = Command::new("git");
        command.args(["-c", "merge.conflictStyle=merge", "merge-file", "-p"]);
        if diff3 {
            command.arg(match style {
                ConflictStyle::Diff3 => "--diff3",
                ConflictStyle::Zdiff3 => "--zdiff3",
            });
        }
        let output = command
            .arg(format!("--marker-size={width}"))
            .args(["-L", &format!("ours: {}", safe_label(&labels.ours))])
            .args(["-L", &format!("base: {}", safe_label(&labels.base))])
            .args(["-L", &format!("theirs: {}", safe_label(&labels.theirs))])
            .args(files.iter().map(|f| f.path()))
            .output()
            .context("无法运行 git merge-file")?;
        match output.status.code() {
            Some(code @ 0..=127) => Ok((String::from_utf8(output.stdout)?, code != 0)),
            _ => bail!(
                "git merge-file 失败：{}",
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    };
    // Plain Git decides the mandatory lower bound. Rendering in another
    // style must never turn its conflict into success.
    let plain = run(false)?;
    if !plain.1 {
        return Ok(plain);
    }
    let diff3 = run(true)?;
    if diff3.1 {
        Ok(diff3)
    } else {
        Ok((
            conflict_box(base, ours, theirs, labels, width, "LINE_CONFLICT"),
            true,
        ))
    }
}

#[derive(Debug)]
pub(crate) struct Part {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) text: String,
    pub(crate) entity: bool,
}

/// Only use a partition when it reproduces every input byte. Nested entities
/// are covered by their outer container in this first implementation.
pub(crate) fn partition(path: &str, text: &str) -> Option<Vec<Part>> {
    // Admit only languages exercised by the initial integration suite.
    if ![".go", ".ts", ".tsx"].iter().any(|ext| path.ends_with(ext)) {
        return None;
    }
    let registry = create_default_registry();
    let plugin = registry.get_explicit_plugin(path)?;
    let (mut entities, tree) = plugin.extract_entities_with_tree(text, path);
    if !text.trim().is_empty() && (tree.as_ref()?.root_node().has_error() || entities.is_empty()) {
        return None;
    }
    entities.sort_by_key(|e| (e.start_line, std::cmp::Reverse(e.end_line)));
    let mut top = Vec::new();
    let mut end = 0;
    for entity in entities {
        if entity.start_line > end {
            end = entity.end_line;
            top.push(entity);
        } else if entity.end_line > end
            || top
                .last()
                .is_some_and(|last: &sem_core::model::entity::SemanticEntity| {
                    last.start_line == entity.start_line && last.end_line == entity.end_line
                })
        {
            return None;
        }
    }
    let regions = extract_regions(text, &top);
    let mut parts = Vec::new();
    let mut seen = BTreeSet::new();
    for region in regions {
        match region {
            FileRegion::Entity(e) => {
                let key = format!("{}:{}", e.entity_type, e.entity_name);
                if !seen.insert(key.clone()) {
                    return None;
                }
                parts.push(Part {
                    key,
                    label: format!("{} {}", e.entity_type, e.entity_name),
                    text: e.content,
                    entity: true,
                });
            }
            FileRegion::Interstitial(gap) => {
                parts.push(Part {
                    key: format!("gap:{}", parts.len()),
                    label: "非实体文本".into(),
                    text: gap.content,
                    entity: false,
                });
            }
        }
    }
    if parts.iter().map(|p| p.text.as_str()).collect::<String>() != text {
        return None;
    }
    Some(parts)
}

fn changed(action: Action) -> bool {
    !matches!(action, Action::Unchanged | Action::Absent)
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::Added => "added",
        Action::Absent => "absent",
        Action::Deleted => "deleted",
        Action::Unchanged => "unchanged",
        Action::Edited => "modified",
        Action::Renamed => "rename candidate",
        Action::RenameEdited => "rename + modified candidate",
    }
}

pub fn merge(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    labels: &Labels,
    width: usize,
) -> Result<Outcome> {
    merge_with_style(
        base,
        ours,
        theirs,
        path,
        labels,
        width,
        ConflictStyle::Diff3,
    )
}

// Only compact the layout fallback when every original byte is unchanged and
// both appended tails contain distinct new entities and whitespace. Other
// layout changes and strict entity conflicts keep their full three-way scope.
fn distinct_entity_appends(base: &str, ours: &str, theirs: &str, path: &str) -> bool {
    let Some(base_parts) = partition(path, base) else {
        return false;
    };
    let base_keys: BTreeSet<_> = base_parts
        .iter()
        .filter(|p| p.entity)
        .map(|p| p.key.as_str())
        .collect();
    let added = |text: &str| -> Option<BTreeSet<String>> {
        let tail = text.strip_prefix(base)?;
        let parts = partition(path, tail)?;
        if parts.iter().any(|p| {
            (!p.entity && !p.text.trim().is_empty())
                || (p.entity && base_keys.contains(p.key.as_str()))
        }) {
            return None;
        }
        let keys: BTreeSet<_> = parts
            .into_iter()
            .filter(|p| p.entity)
            .map(|p| p.key)
            .collect();
        (!keys.is_empty()).then_some(keys)
    };
    matches!((added(ours), added(theirs)), (Some(o), Some(t)) if o.is_disjoint(&t))
}

pub fn merge_with_style(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    labels: &Labels,
    width: usize,
    style: ConflictStyle,
) -> Result<Outcome> {
    validate_inputs([base, ours, theirs], width)?;
    let (line_content, line_conflict) = line_merge(base, ours, theirs, labels, width, style)?;
    let upstream = weave_core::entity_merge(base, ours, theirs, path);
    let mut reasons = Vec::new();
    if line_conflict {
        reasons.push("LINE_CONFLICT：Git 行级冲突".into());
    }
    for conflict in &upstream.conflicts {
        reasons.push(format!(
            "WEAVE_REFUSAL：{} {} [{}]",
            conflict.entity_type,
            conflict.entity_name,
            conflict.kind.wire_kind()
        ));
    }

    // A byte-identical final pair is NOT a clean fast path: both can have
    // changed the same entity relative to base.
    let both_changed = ours != base && theirs != base;
    let parts = (
        partition(path, base),
        partition(path, ours),
        partition(path, theirs),
    );
    let reliable = parts.0.is_some() && parts.1.is_some() && parts.2.is_some();
    if both_changed {
        if !reliable {
            reasons.push("ENTITY_ANALYSIS_UNAVAILABLE：无法可靠分析，保留整文件冲突".into());
        } else if let Some(analysis) = analyze_default(base, ours, theirs, path) {
            for (triple, cell) in analysis.iter() {
                let (o, t) = cell.actions();
                if changed(o) && changed(t) {
                    reasons.push(format!(
                        "ENTITY_BOTH_CHANGED：{} [ours={}, theirs={}]",
                        analysis.label(triple),
                        action_label(o),
                        action_label(t)
                    ));
                }
            }
        } else {
            reasons.push("ENTITY_ANALYSIS_UNAVAILABLE：weave 无实体分类".into());
        }
    }

    // For unchanged entity order and exact byte partitions, render the whole
    // conflicting entity while merging independent text outside that entity.
    if let (Some(b), Some(o), Some(t)) = &parts {
        let same_keys =
            |x: &[Part], y: &[Part]| x.iter().map(|p| &p.key).eq(y.iter().map(|p| &p.key));
        if same_keys(b, o) && same_keys(b, t) {
            let mut composed = String::new();
            let mut entity_conflict = false;
            for ((bp, op), tp) in b.iter().zip(o).zip(t) {
                if bp.text != op.text && bp.text != tp.text {
                    entity_conflict = true;
                    let reason = if bp.entity {
                        format!("ENTITY_BOTH_CHANGED：{}", bp.label)
                    } else {
                        "UNMODELED_BOTH_CHANGED：双方修改同一非实体区域".into()
                    };
                    reasons.push(reason.clone());
                    composed.push_str(&conflict_box(
                        &bp.text, &op.text, &tp.text, labels, width, &reason,
                    ));
                } else {
                    let (content, conflict) =
                        line_merge(&bp.text, &op.text, &tp.text, labels, width, style)?;
                    if conflict {
                        reasons.push(format!("LINE_CONFLICT：{}", bp.label));
                    }
                    composed.push_str(&content);
                }
            }
            // Keep every original line conflict, even when region splitting
            // would resolve it. Upstream refusals may concern multiple regions.
            if entity_conflict && !line_conflict && upstream.conflicts.is_empty() {
                reasons.sort();
                reasons.dedup();
                return Ok(Outcome {
                    content: composed,
                    reasons,
                });
            }
        } else if both_changed {
            reasons.push("ENTITY_LAYOUT_CHANGED：实体增删或顺序变化，保留整文件冲突".into());
        }
    }

    let compact_appends = style == ConflictStyle::Zdiff3
        && line_conflict
        && reliable
        && reasons
            .iter()
            .all(|r| r.starts_with("LINE_CONFLICT：") || r.starts_with("ENTITY_LAYOUT_CHANGED："))
        && distinct_entity_appends(base, ours, theirs, path);
    if compact_appends {
        for reason in &mut reasons {
            if reason.starts_with("ENTITY_LAYOUT_CHANGED：") {
                *reason = "ENTITY_LAYOUT_CHANGED：双方在文件末尾新增不同实体，保留行级冲突".into();
            }
        }
    }
    reasons.sort();
    reasons.dedup();
    let content = if reasons.is_empty() || (reasons.len() == 1 && line_conflict) || compact_appends
    {
        line_content
    } else {
        // Avoid trying to splice separately owned entity and line conflict
        // ranges. This conservative fallback cannot lose either side's text.
        conflict_box(base, ours, theirs, labels, width, &reasons.join("; "))
    };
    Ok(Outcome { content, reasons })
}
