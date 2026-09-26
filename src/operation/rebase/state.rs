//! Durable rebase phases. An interrupted apply must never look like a staged
//! resolution; ref updates are recorded before they happen and use expected OIDs.
use super::super::{
    git::{commit_id, ensure_no_git_operation, git_path, head_branch, read, repository_root},
    plan::{read_json, MAX_ARTIFACT},
    Options,
};
use crate::repository;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(super) const VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Phase {
    Starting,
    Ready,
    Applying,
    Applied,
    Committing { commit: String },
    Finishing,
    Aborting { head: String, branch_tip: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub version: u32,
    pub repository: String,
    pub branch: String,
    pub original_head: String,
    pub upstream: String,
    pub commits: Vec<String>,
    pub next: usize,
    pub head: String,
    pub options: Options,
    pub phase: Phase,
}

pub(super) fn path() -> Result<PathBuf> {
    git_path("strict-weave/rebase-state.json")
}

impl State {
    pub fn save(&self) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_ARTIFACT {
            bail!("rebase 状态超过 64 MiB");
        }
        repository::atomic_write(&path()?, &bytes)
    }

    pub fn load() -> Result<Self> {
        let state: Self =
            read_json(&path()?).context("无法读取当前版本的 strict-weave rebase 状态")?;
        if state.version != VERSION
            || state.repository != repository_root()?.display().to_string()
            || !state.branch.starts_with("refs/heads/")
            || state.next > state.commits.len()
            || (matches!(state.phase, Phase::Starting) && state.next != 0)
            || (matches!(
                state.phase,
                Phase::Applying | Phase::Applied | Phase::Committing { .. }
            ) && state.next == state.commits.len())
            || (matches!(state.phase, Phase::Finishing) && state.next != state.commits.len())
        {
            bail!("strict-weave rebase 状态不完整或不属于当前仓库");
        }
        state.validate_context()?;
        Ok(state)
    }

    /// Only Starting/Finishing/Aborting may have HEAD attached to the original
    /// branch. Switching to any other branch, even at the same OID, is rejected.
    pub fn validate_context(&self) -> Result<()> {
        let head = commit_id("HEAD")?;
        let branch = head_branch()?;
        let branch_tip = commit_id(&self.branch)?;
        let (head_ok, tip_ok, attached_ok) = match &self.phase {
            Phase::Starting => (
                head == self.head || head == self.original_head,
                branch_tip == self.original_head,
                head == self.original_head,
            ),
            Phase::Finishing => (
                head == self.head,
                branch_tip == self.original_head || branch_tip == self.head,
                branch_tip == self.head,
            ),
            Phase::Aborting {
                head: before,
                branch_tip: before_tip,
            } => (
                head == *before || head == self.original_head,
                branch_tip == *before_tip || branch_tip == self.original_head,
                // Finishing may have already reattached HEAD before abort
                // started. A failed reset can leave that recorded tip intact.
                head == branch_tip,
            ),
            Phase::Committing { commit } => (
                head == self.head || head == *commit,
                branch_tip == self.original_head,
                false,
            ),
            _ => (head == self.head, branch_tip == self.original_head, false),
        };
        if !head_ok
            || !tip_ok
            || branch
                .as_ref()
                .is_some_and(|b| !attached_ok || b != &self.branch)
        {
            bail!("rebase 的 HEAD 或原 branch 已变化；请恢复记录的 HEAD/branch 后重试，状态已保留");
        }
        let allowed_cherry = if matches!(self.phase, Phase::Applying | Phase::Aborting { .. }) {
            self.commits.get(self.next).map(String::as_str)
        } else {
            None
        };
        ensure_no_git_operation(allowed_cherry)
    }

    pub fn remove(self) -> Result<()> {
        std::fs::remove_file(path()?).context("清理 strict-weave rebase 状态")
    }
}

pub(super) fn ensure_resolved() -> Result<()> {
    if !read(&["ls-files", "-u", "-z"])?.is_empty() {
        bail!("仍有未解决的 index 冲突；请先编辑并 git add 文件");
    }
    // Separate queries also handle staged renames and paths containing spaces,
    // tabs or newlines without parsing porcelain's variable-length records.
    if !read(&["diff-files", "--name-only", "-z"])?.is_empty()
        || !read(&["ls-files", "--others", "--exclude-standard", "-z"])?.is_empty()
    {
        bail!("存在未暂存或未跟踪的修改；请只保留已 git add 的 rebase 结果");
    }
    Ok(())
}
