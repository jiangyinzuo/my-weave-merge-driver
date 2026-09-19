use crate::reason::{self, Change, Evidence, Kind, MoveEvidence, Reason, Subject, WeaveSource};
use anyhow::{bail, Context, Result};
use sem_core::parser::plugins::create_default_registry;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::Command,
};
use weave_core::{
    region::{extract_regions, FileRegion},
    v2::analyze_default,
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
    pub reasons: Vec<Reason>,
    /// 非阻断的跨文件关联；有候选不等于有冲突。
    pub related_moves: Vec<MoveEvidence>,
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
    name: String,
    entity_type: String,
    pub(crate) label: String,
    pub(crate) text: String,
    pub(crate) entity: bool,
}

/// Only use a partition when it reproduces every input byte. Nested entities
/// are covered by their outer container in this first implementation.
pub(crate) fn partition(path: &str, text: &str) -> Option<Vec<Part>> {
    // Use the same grammar registry as weave; do not maintain a second
    // extension allowlist. Reliability checks below apply to every language.
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
                    name: e.entity_name,
                    entity_type: e.entity_type,
                    text: e.content,
                    entity: true,
                });
            }
            FileRegion::Interstitial(gap) => {
                parts.push(Part {
                    key: format!("gap:{}", parts.len()),
                    name: String::new(),
                    entity_type: String::new(),
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

impl Part {
    fn subject(&self) -> Subject {
        if self.entity {
            Subject::Entity {
                entity_type: self.entity_type.clone(),
                name: self.name.clone(),
            }
        } else {
            Subject::Gap {
                key: self.key.clone(),
            }
        }
    }
}

fn same_keys(x: &[Part], y: &[Part]) -> bool {
    x.iter().map(|p| &p.key).eq(y.iter().map(|p| &p.key))
}

// The public weave classification exposes a name, not a full identity. Only
// bind it when all three partitions are reliable and agree on a unique target.
type ThreePartitions = (Option<Vec<Part>>, Option<Vec<Part>>, Option<Vec<Part>>);

fn local_subject(
    parts: &ThreePartitions,
    name: &str,
    entity_type: Option<&str>,
) -> Option<Subject> {
    let (Some(b), Some(o), Some(t)) = parts else {
        return None;
    };
    let mut candidates = BTreeSet::new();
    for version in [b, o, t] {
        let found: Vec<_> = version
            .iter()
            .filter(|p| {
                p.entity && p.name == name && entity_type.is_none_or(|kind| p.entity_type == kind)
            })
            .collect();
        if found.len() > 1 {
            return None;
        }
        candidates.extend(found.into_iter().map(Part::subject));
    }
    if candidates.len() == 1 {
        candidates.pop_first()
    } else {
        None
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
        reasons.push(Reason::new(
            Kind::LineConflict,
            Subject::File,
            Evidence::GitLineConflict,
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
    for (ordinal, conflict) in upstream.conflicts.iter().enumerate() {
        let subject = local_subject(&parts, &conflict.entity_name, Some(&conflict.entity_type))
            .unwrap_or_else(|| Subject::Weave {
                name: conflict.entity_name.clone(),
                source: WeaveSource::Refusal,
                ordinal,
            });
        reasons.push(Reason::new(
            Kind::EntityConflict,
            subject,
            Evidence::WeaveRefusal {
                refusal: (&conflict.kind).into(),
            },
        ));
    }

    if both_changed {
        if !reliable {
            reasons.push(Reason::new(
                Kind::AnalysisUnavailable,
                Subject::File,
                Evidence::PartitionUnavailable,
            ));
        } else if let Some(analysis) = analyze_default(base, ours, theirs, path) {
            let mut counts = BTreeMap::new();
            for (triple, _) in analysis.iter() {
                *counts.entry(analysis.label(triple)).or_insert(0) += 1;
            }
            for (ordinal, (triple, cell)) in analysis.iter().enumerate() {
                let (o, t) = cell.actions();
                let (ours, theirs) = (Change::from(o), Change::from(t));
                if ours.changed() && theirs.changed() {
                    let name = analysis.label(triple);
                    let subject = if counts[&name] == 1 {
                        local_subject(&parts, &name, None)
                    } else {
                        None
                    }
                    .unwrap_or(Subject::Weave {
                        name,
                        source: WeaveSource::Classification,
                        ordinal,
                    });
                    // weave already owns refusal rules. Add our strict policy
                    // only when this reliably identified entity has no refusal.
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
        } else {
            reasons.push(Reason::new(
                Kind::AnalysisUnavailable,
                Subject::File,
                Evidence::WeaveUnavailable,
            ));
        }
    }

    // For unchanged entity order and exact byte partitions, render the whole
    // conflicting entity while merging independent text outside that entity.
    if let (Some(b), Some(o), Some(t)) = &parts {
        if same_keys(b, o) && same_keys(b, t) {
            let mut composed = String::new();
            let mut entity_conflict = false;
            for ((bp, op), tp) in b.iter().zip(o).zip(t) {
                let subject = bp.subject();
                let existing = reasons
                    .iter()
                    .find(|r| r.kind == Kind::EntityConflict && r.subject == subject)
                    .cloned();
                // Reuse the upstream decision/classification for rendering too.
                // Only uncovered regions need byte-level strict checks: weave
                // normalizes encoding, and does not classify interstitial text.
                let conflict = existing.or_else(|| {
                    if bp.text == op.text || bp.text == tp.text {
                        return None;
                    }
                    let reason = Reason::new(
                        if bp.entity {
                            Kind::EntityConflict
                        } else {
                            Kind::NonEntityConflict
                        },
                        subject,
                        Evidence::RawBothChanged,
                    );
                    reasons.push(reason.clone());
                    Some(reason)
                });
                if let Some(reason) = conflict {
                    entity_conflict = true;
                    composed.push_str(&conflict_box(
                        &bp.text,
                        &op.text,
                        &tp.text,
                        labels,
                        width,
                        &reason.summary(),
                    ));
                } else {
                    let (content, conflict) =
                        line_merge(&bp.text, &op.text, &tp.text, labels, width, style)?;
                    if conflict {
                        reasons.push(Reason::new(
                            Kind::LineConflict,
                            bp.subject(),
                            Evidence::GitLineConflict,
                        ));
                    }
                    composed.push_str(&content);
                }
            }
            // Keep every original line conflict, even when region splitting
            // would resolve it. Upstream refusals may concern multiple regions.
            if entity_conflict && !line_conflict && upstream.conflicts.is_empty() {
                reason::normalize(&mut reasons);
                return Ok(Outcome {
                    content: composed,
                    reasons,
                    related_moves: Vec::new(),
                });
            }
        } else if both_changed {
            reasons.push(Reason::new(
                Kind::LayoutChanged,
                Subject::File,
                Evidence::LayoutChanged,
            ));
        }
    }

    let compact_appends = style == ConflictStyle::Zdiff3
        && line_conflict
        && reliable
        && reasons
            .iter()
            .all(|r| matches!(r.kind, Kind::LineConflict | Kind::LayoutChanged))
        && distinct_entity_appends(base, ours, theirs, path);
    reason::normalize(&mut reasons);
    let content = if reasons.is_empty() || (reasons.len() == 1 && line_conflict) || compact_appends
    {
        line_content
    } else {
        // Avoid trying to splice separately owned entity and line conflict
        // ranges. This conservative fallback cannot lose either side's text.
        conflict_box(
            base,
            ours,
            theirs,
            labels,
            width,
            &reason::summaries(&reasons),
        )
    };
    Ok(Outcome {
        content,
        reasons,
        related_moves: Vec::new(),
    })
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn part(kind: &str, name: &str) -> Part {
        Part {
            key: format!("{kind}:{name}"),
            name: name.into(),
            entity_type: kind.into(),
            label: format!("{kind} {name}"),
            text: String::new(),
            entity: true,
        }
    }
    #[test]
    fn public_name_cannot_bind_ambiguous_types_or_unreliable_versions() {
        let parts = (
            Some(vec![part("function", "f"), part("interface", "f")]),
            Some(vec![part("function", "f")]),
            Some(vec![part("interface", "f")]),
        );
        assert!(local_subject(&parts, "f", None).is_none());
        assert_eq!(
            local_subject(&parts, "f", Some("function")),
            Some(part("function", "f").subject())
        );
        let parts = (
            Some(vec![part("function", "f")]),
            None,
            Some(vec![part("function", "f")]),
        );
        assert!(local_subject(&parts, "f", Some("function")).is_none());
    }
    #[test]
    fn type_changes_are_not_merged_but_confirmed_modify_delete_can_share_parent() {
        let parts = (
            Some(vec![part("function", "f")]),
            Some(vec![part("class", "f")]),
            Some(vec![part("function", "f")]),
        );
        assert!(local_subject(&parts, "f", None).is_none());
        let parts = (
            Some(vec![part("function", "f")]),
            Some(vec![]),
            Some(vec![part("function", "f")]),
        );
        assert_eq!(
            local_subject(&parts, "f", None),
            Some(part("function", "f").subject())
        );
    }
}
