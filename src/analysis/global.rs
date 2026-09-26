//! Explicit three-tree analysis. Operation commands read Git objects first and
//! then apply the resulting findings to the standard Git index/worktree state.
use super::moves::{conflict_reason, entities, find_candidates, ParsedSnapshots};
use crate::merge::{self, ConflictStyle, Labels, Outcome};
pub use crate::reason::Location;
use crate::reason::{self, MoveEvidence, Reason};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};

pub(super) const MAX_TOTAL: usize = 64 * 1024 * 1024;
const ENGINE: &str = "strict-weave-global-v9";
pub type TreeSnapshot = BTreeMap<String, String>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub engine: String,
    /// Always base, ours, theirs; resolved trees, never guessed merge bases.
    pub trees: [String; 3],
    pub files: BTreeMap<String, FileAnalysis>,
    pub move_candidates: Vec<MoveCandidate>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn conflicted(&self) -> bool {
        self.files.values().any(|f| !f.reasons.is_empty())
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 5 || self.engine != ENGINE {
            bail!("分析报告版本不兼容");
        }
        if self.files.values().any(|file| {
            file.reasons.iter().any(|reason| !reason.valid())
                || file
                    .related_moves
                    .iter()
                    .any(|candidate| !candidate.valid())
        }) || self
            .move_candidates
            .iter()
            .any(|candidate| !candidate.valid())
        {
            bail!("无效的分析原因结构");
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAnalysis {
    /// SHA-256 of raw bytes in base, ours, theirs. None means path absent.
    pub fingerprints: [Option<String>; 3],
    pub reasons: Vec<Reason>,
    pub related_moves: Vec<MoveEvidence>,
}

pub type MoveCandidate = MoveEvidence;

fn fingerprint(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Text snapshots are internal/library inputs, not a public diff command.
/// Missing paths and present empty files remain distinct in the report.
pub fn analyze(snapshots: &[TreeSnapshot; 3], trees: [String; 3]) -> Result<Report> {
    let mut total = 0usize;
    for snapshot in snapshots {
        for (path, text) in snapshot {
            if path.is_empty() {
                bail!("空路径不受支持");
            }
            merge::validate_text(text.as_bytes()).with_context(|| format!("读取 {path}"))?;
            total += text.len();
            if total > MAX_TOTAL {
                bail!("三方分析文本超过 64 MiB");
            }
        }
    }
    let mut report = Report {
        schema_version: 5,
        engine: ENGINE.into(),
        trees,
        files: BTreeMap::new(),
        move_candidates: Vec::new(),
        warnings: Vec::new(),
    };
    let paths: BTreeSet<_> = snapshots.iter().flat_map(|s| s.keys()).collect();
    let mut parsed: ParsedSnapshots = Default::default();
    for path in paths {
        let texts = snapshots.each_ref().map(|s| s.get(path));
        if texts[0] == texts[1] && texts[0] == texts[2] {
            continue;
        }
        let contents = texts.map(|t| t.map(String::as_str).unwrap_or(""));
        // Same original-byte baseline and entity rules as operation rendering.
        let local = super::local::analyze(
            contents[0],
            contents[1],
            contents[2],
            path,
            &Labels::default(),
            7,
            ConflictStyle::Diff3,
        )
        .with_context(|| format!("分析 {path}"))?;
        report.files.insert(
            path.clone(),
            FileAnalysis {
                fingerprints: texts.map(|t| t.map(|s| fingerprint(s))),
                reasons: local.reasons,
                related_moves: Vec::new(),
            },
        );
        let (base, ours, theirs) = local.partitions;
        for (side, partitions) in [base, ours, theirs].into_iter().enumerate() {
            let parts = if texts[side].is_some() {
                partitions.map(|parts| entities(path, contents[side], parts))
            } else {
                Some(BTreeMap::new())
            };
            if parts.is_none() {
                report.warnings.push(format!(
                    "ENTITY_ANALYSIS_UNAVAILABLE：{}:{path}；移动关联不完整，单文件严格检查仍执行",
                    ["base", "ours", "theirs"][side]
                ));
            }
            parsed[side].insert(path.clone(), parts);
        }
    }
    report.move_candidates = find_candidates(&parsed)?;
    for candidate in &report.move_candidates {
        let reason = conflict_reason(candidate);
        for path in [&candidate.base.path, &candidate.target.path] {
            let file = report.files.get_mut(path).expect("changed candidate path");
            file.related_moves.push(candidate.clone());
            if let Some(reason) = &reason {
                file.reasons.push(reason.clone());
            }
        }
    }
    for file in report.files.values_mut() {
        reason::normalize(&mut file.reasons);
        file.related_moves.sort();
        file.related_moves.dedup();
    }
    report.warnings.sort();
    report.warnings.dedup();
    Ok(report)
}

/// Atomically creates a new read-only result; no replacement of existing files.
pub fn save(path: &Path, report: &Report) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(report)?;
    if bytes.len() + 1 > MAX_TOTAL {
        bail!("分析结果超过 64 MiB");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    let mut permissions = file.as_file().metadata()?.permissions();
    permissions.set_readonly(true);
    file.as_file().set_permissions(permissions)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|e| e.error)
        .context("无法新建分析文件；不覆盖已有结果")?;
    Ok(())
}

/// Read one path from a serialized report and verify its three raw inputs.
/// Operation code uses this same invariant when it reconstructs a plan.
pub fn load_file_analysis(path: &Path, source: &str, texts: [&str; 3]) -> Result<FileAnalysis> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .context("读取分析报告")?
        .take((MAX_TOTAL + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TOTAL {
        bail!("分析报告超过 64 MiB");
    }
    let mut report: Report = serde_json::from_slice(&bytes).context("无效的分析报告")?;
    report.validate()?;
    let file = report
        .files
        .remove(source)
        .context("分析报告未包含此路径")?;
    for side in 0..3 {
        let matches = match &file.fingerprints[side] {
            Some(expected) => *expected == fingerprint(texts[side]),
            None => texts[side].is_empty(),
        };
        if !matches {
            bail!(
                "分析输入不匹配：{}；报告可能已过期",
                ["base", "ours", "theirs"][side]
            );
        }
    }
    Ok(file)
}

pub fn apply_file_analysis(
    outcome: &mut Outcome,
    global: &FileAnalysis,
    texts: [&str; 3],
    labels: &Labels,
    width: usize,
) {
    let already_conflicted = outcome.conflicted();
    outcome.reasons.extend(global.reasons.iter().cloned());
    reason::normalize(&mut outcome.reasons);
    outcome
        .related_moves
        .extend(global.related_moves.iter().cloned());
    outcome.related_moves.sort();
    outcome.related_moves.dedup();
    if !outcome.reasons.is_empty() && !already_conflicted {
        outcome.content = merge::conflict_box(
            texts[0],
            texts[1],
            texts[2],
            labels,
            width,
            &reason::summaries(&outcome.reasons),
        );
    }
    if outcome.conflicted() {
        outcome.content = merge::annotate_moves(&outcome.content, &outcome.related_moves, width);
    }
}
