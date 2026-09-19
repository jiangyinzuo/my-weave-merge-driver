//! File output and human-readable driver diagnostics. No Git operation orchestration.
use crate::merge::{self, Labels, Outcome};
use crate::reason::{Evidence, MoveEvidence, MoveMatch, Reason};
use anyhow::{bail, Context, Result};
use std::{io::Write, path::Path};

pub fn report(path: &str, outcome: &Outcome, labels: &Labels, detailed: bool) {
    if outcome.conflicted() {
        eprintln!("冲突 · {}", merge::safe_label(path));
        eprintln!(
            "ours   : {}\nbase   : {}\ntheirs : {}",
            merge::safe_label(&labels.ours),
            merge::safe_label(&labels.base),
            merge::safe_label(&labels.theirs)
        );
        for reason in &outcome.reasons {
            for line in reason.lines(detailed) {
                eprintln!("{line}");
            }
        }
    }
    for line in related_move_lines(&outcome.reasons, &outcome.related_moves, detailed) {
        eprintln!("{line}");
    }
    if outcome.conflicted() {
        eprintln!("需人工处理：选择一侧 / 编辑合并结果。\n");
    }
}

/// 未阻断的 rename 也应可检视；已挂在原因下的关联不重复输出。
pub fn related_move_lines(
    reasons: &[Reason],
    candidates: &[MoveEvidence],
    detailed: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    for candidate in candidates {
        if reasons.is_empty() && matches!(candidate.matched_by, MoveMatch::Exact) {
            continue;
        }
        let evidence = Evidence::MoveCandidate {
            candidate: Box::new(candidate.clone()),
        };
        if !reasons.iter().any(|r| r.evidence.contains(&evidence)) {
            lines.extend(
                evidence
                    .lines(detailed)
                    .into_iter()
                    .map(|line| format!("关联 {line}")),
            );
        }
    }
    lines
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_file() {
            bail!("拒绝覆盖非普通文件：{}", path.display());
        }
        tmp.as_file().set_permissions(metadata.permissions())?;
    }
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| e.error)
        .context("无法写入合并结果")?;
    Ok(())
}
