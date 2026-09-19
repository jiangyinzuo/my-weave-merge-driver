//! File output and human-readable driver diagnostics. No Git operation orchestration.
use crate::merge::{self, Labels, Outcome};
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
        for candidate in &outcome.related_moves {
            let evidence = crate::reason::Evidence::MoveCandidate {
                candidate: candidate.clone(),
            };
            if !outcome
                .reasons
                .iter()
                .any(|reason| reason.evidence.contains(&evidence))
            {
                eprintln!("关联 {}", evidence.summary());
            }
        }
        eprintln!("需人工处理：选择一侧 / 编辑合并结果。\n");
    }
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
