//! Git 行级基线：冲突是严格合并必须保留的下限。
use crate::merge::{safe_label, ConflictStyle, Labels};
use anyhow::{bail, Context, Result};
use std::{io::Write, process::Command};

/// Run Git once on the original text, independently of weave. Git's diff3 and
/// zdiff3 retain the initial conflict modes; plain merge may further resolve
/// them. Thus plain conflicts are a subset of either style's conflicts.
/// See docs/diff3-vs-zdiff3.md for the source audit and its assumptions.
pub(super) fn line_merge(
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
    let output = Command::new("git")
        .args(["-c", "merge.conflictStyle=merge", "merge-file", "-p"])
        .arg(match style {
            ConflictStyle::Diff3 => "--diff3",
            ConflictStyle::Zdiff3 => "--zdiff3",
        })
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
}
