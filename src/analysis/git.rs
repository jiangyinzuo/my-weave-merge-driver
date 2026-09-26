//! Read-only Git tree inputs. Keep blob IDs and modes beside the text snapshot
//! so conflict stages use those same objects without rescanning the tree.
use super::global::{TreeSnapshot, MAX_TOTAL};
use crate::merge;
use anyhow::{bail, Context, Result};
use std::{collections::BTreeMap, process::Command};

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

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub mode: String,
    pub oid: String,
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
    // Without --full-tree, Git silently scopes paths to the caller's cwd.
    // Global analysis must also see conflicts outside that subdirectory.
    let bytes = git(&["ls-tree", "--full-tree", "-r", "-z", &id])?;
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

/// Read a complete UTF-8 text snapshot from a revision without touching the
/// index or worktree. The operation layer uses this before invoking Git so
/// global analysis sees paths that Git may later skip.
pub(crate) struct GitSnapshot {
    pub tree_id: String,
    pub texts: TreeSnapshot,
    pub entries: BTreeMap<String, Entry>,
}

impl GitSnapshot {
    pub fn load(revision: &str) -> Result<Self> {
        let (tree_id, entries) = tree(revision)?;
        let mut texts = BTreeMap::new();
        let mut total = 0usize;
        for (path, entry) in &entries {
            if !matches!(entry.mode.as_str(), "100644" | "100755") {
                bail!("初版分析不支持 symlink/submodule：{path}");
            }
            let size = String::from_utf8(git(&["cat-file", "-s", &entry.oid])?)?
                .trim()
                .parse::<usize>()?;
            total = total.checked_add(size).context("分析输入大小溢出")?;
            if size > merge::MAX_BYTES || total > MAX_TOTAL {
                bail!("分析文本超过大小限制：{path}");
            }
            let bytes = git(&["cat-file", "blob", &entry.oid])?;
            texts.insert(path.clone(), merge::validate_text(&bytes)?.to_owned());
        }
        Ok(Self {
            tree_id,
            texts,
            entries,
        })
    }
}

/// Read the complete text tree without touching the index or worktree.
pub fn snapshot(revision: &str) -> Result<(String, TreeSnapshot)> {
    let loaded = GitSnapshot::load(revision)?;
    Ok((loaded.tree_id, loaded.texts))
}

pub fn blob_oid(revision: &str, path: &str) -> Result<Option<(String, String)>> {
    let (_, entries) = tree(revision)?;
    Ok(entries
        .get(path)
        .map(|entry| (entry.mode.clone(), entry.oid.clone())))
}
