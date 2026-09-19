//! Explicit three-tree analysis. This module never runs or controls a Git merge.
//! Cached findings can only add conflicts; every driver call still checks its inputs.
use super::moves::{conflict_reason, entities, find_candidates, ParsedSnapshots};
use crate::merge::{self, Labels, Outcome};
pub use crate::reason::Location;
use crate::reason::{self, MoveEvidence, Reason};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    process::Command,
};

const MAX_TOTAL: usize = 64 * 1024 * 1024;
const ENGINE: &str = "strict-weave-global-v6";
type Snapshot = BTreeMap<String, String>;

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
pub fn analyze(snapshots: &[Snapshot; 3], trees: [String; 3]) -> Result<Report> {
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
        schema_version: 2,
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
        // Same original-byte baseline and entity rules as the actual driver.
        let outcome = merge::merge(
            contents[0],
            contents[1],
            contents[2],
            path,
            &Labels::default(),
            7,
        )
        .with_context(|| format!("分析 {path}"))?;
        report.files.insert(
            path.clone(),
            FileAnalysis {
                fingerprints: texts.map(|t| t.map(|s| fingerprint(s))),
                reasons: outcome.reasons,
                related_moves: Vec::new(),
            },
        );
        for side in 0..3 {
            let parts = if texts[side].is_some() {
                entities(path, contents[side])
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

fn git(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .context("无法运行 git")?;
    if !output.status.success() {
        bail!(
            "git {} 失败：{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

#[derive(PartialEq, Eq)]
struct Entry {
    mode: String,
    oid: String,
}

fn tree(revision: &str) -> Result<(String, BTreeMap<String, Entry>)> {
    let id = String::from_utf8(git(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{revision}^{{tree}}"),
    ])?)?
    .trim_end()
    .to_owned();
    let bytes = git(&["ls-tree", "-r", "-z", &id])?;
    let mut entries = BTreeMap::new();
    for row in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row).context("初版不支持非 UTF-8 路径")?;
        let (meta, path) = row.split_once('\t').context("无效的 ls-tree 输出")?;
        let fields: Vec<_> = meta.split_whitespace().collect();
        if fields.len() != 3 {
            bail!("无效的 ls-tree 元数据");
        }
        entries.insert(
            path.into(),
            Entry {
                mode: fields[0].into(),
                oid: fields[2].into(),
            },
        );
    }
    Ok((id, entries))
}

/// Only reads trees/blobs. Does not infer operation context from HEAD/state files,
/// and does not invoke external diff, filters, hooks, merge or index writes.
pub fn prepare(revisions: [&str; 3]) -> Result<Report> {
    let trees = [
        tree(revisions[0])?,
        tree(revisions[1])?,
        tree(revisions[2])?,
    ];
    let mut snapshots: [Snapshot; 3] = Default::default();
    let paths: BTreeSet<_> = trees
        .iter()
        .flat_map(|(_, entries)| entries.keys())
        .collect();
    let mut total = 0usize;
    let mut metadata_only = Vec::new();
    for path in paths {
        let entries = trees.each_ref().map(|(_, e)| e.get(path));
        if entries[0] == entries[1] && entries[0] == entries[2] {
            continue;
        }
        for side in 0..3 {
            if let Some(entry) = entries[side] {
                if !matches!(entry.mode.as_str(), "100644" | "100755") {
                    bail!("初版分析不支持变化的 symlink/submodule：{path}");
                }
                let size = String::from_utf8(git(&["cat-file", "-s", &entry.oid])?)?
                    .trim()
                    .parse::<usize>()?;
                total = total.checked_add(size).context("分析输入大小溢出")?;
                if size > merge::MAX_BYTES || total > MAX_TOTAL {
                    bail!("分析文本超过大小限制：{path}");
                }
                let bytes = git(&["cat-file", "blob", &entry.oid])?;
                snapshots[side].insert(
                    path.clone(),
                    merge::validate_text(&bytes)
                        .with_context(|| format!("读取 {path}"))?
                        .to_owned(),
                );
            }
        }
        metadata_only.push(path.clone());
    }
    let mut report = analyze(&snapshots, trees.each_ref().map(|(id, _)| id.clone()))?;
    // Include fingerprints even for mode-only changes: the driver can still be
    // called there. Git, not this content driver, owns mode conflict handling.
    for path in metadata_only {
        report
            .files
            .entry(path.clone())
            .or_insert_with(|| FileAnalysis {
                fingerprints: snapshots
                    .each_ref()
                    .map(|s| s.get(&path).map(|t| fingerprint(t))),
                reasons: Vec::new(),
                related_moves: Vec::new(),
            });
    }
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

pub fn load_for_driver(path: &Path, source: &str, texts: [&str; 3]) -> Result<FileAnalysis> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .context("读取全局分析文件")?
        .take((MAX_TOTAL + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TOTAL {
        bail!("全局分析文件超过 64 MiB");
    }
    let mut report: Report = serde_json::from_slice(&bytes).context("无效的全局分析文件")?;
    if report.schema_version != 2 || report.engine != ENGINE {
        bail!("全局分析版本不兼容");
    }
    if report.files.values().any(|file| {
        file.reasons.iter().any(|reason| !reason.valid())
            || file.related_moves.iter().any(|c| !c.valid())
    }) || report.move_candidates.iter().any(|c| !c.valid())
    {
        bail!("无效的全局分析原因结构");
    }
    let file = report
        .files
        .remove(source)
        .context("全局分析未包含此路径；不能确认上下文，拒绝使用")?;
    for side in 0..3 {
        let matches = match &file.fingerprints[side] {
            Some(expected) => *expected == fingerprint(texts[side]),
            None => texts[side].is_empty(),
        };
        if !matches {
            bail!(
                "全局分析输入不匹配：{}；可能过期、路径变化、虚拟 base 或输入转换；保持 ours 不变",
                ["base", "ours", "theirs"][side]
            );
        }
    }
    Ok(file)
}

pub fn augment(
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
}
