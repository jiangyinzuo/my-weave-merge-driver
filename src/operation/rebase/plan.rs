//! Rebase preview: analyze each simulated full-tree replay until the first
//! conflict. Later commits stay pending because their ours tree is not known.
use super::super::{
    git::{clean, commit_id, head_branch, index_sha256, native_output, repository_root, string},
    plan::{read_json, save_new, Artifact},
};
use super::{replay_plan, state::State};
use crate::repository;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    analysis: Artifact,
    native_conflicted: bool,
    result_tree: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    version: u32,
    operation: String,
    target: String,
    repository: String,
    original_head: String,
    branch: String,
    upstream: String,
    index_sha256: String,
    commits: Vec<String>,
    steps: Vec<Step>,
    pending: Vec<String>,
}

impl Preview {
    pub fn build(target: &str, state: &State) -> Result<Self> {
        let mut preview = Self {
            version: 2,
            operation: "rebase".into(),
            target: target.into(),
            repository: state.repository.clone(),
            original_head: state.original_head.clone(),
            branch: state.branch.clone(),
            upstream: state.upstream.clone(),
            index_sha256: index_sha256()?,
            commits: state.commits.clone(),
            steps: Vec::new(),
            pending: Vec::new(),
        };
        let mut ours = state.upstream.clone();
        for commit in &state.commits {
            let plan = replay_plan(&ours, commit, state.options)?;
            // merge-tree only writes immutable Git objects. HEAD, refs, index
            // and worktree are untouched, even for a native conflict.
            let output = native_output(&[
                "merge-tree",
                "--write-tree",
                "--no-messages",
                "-z",
                &format!("--merge-base={}", plan.artifact.revisions[0]),
                &ours,
                commit,
            ])?;
            let native_conflicted = match output.status.code() {
                Some(0) => false,
                Some(1) => true,
                _ => bail!(
                    "无法预演 rebase：{}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            };
            let tree =
                std::str::from_utf8(output.stdout.split(|b| *b == 0).next().unwrap_or_default())?;
            let conflicted = native_conflicted || !plan.outcomes.is_empty();
            // Validate Git's output as a tree before using it in the next step.
            let tree = string(&[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{tree}^{{tree}}"),
            ])?;
            preview.steps.push(Step {
                analysis: plan.artifact,
                native_conflicted,
                result_tree: (!conflicted).then(|| tree.clone()),
            });
            if conflicted {
                break;
            }
            ours = tree;
        }
        preview.pending = state.commits[preview.steps.len()..].to_vec();
        preview.recheck()?;
        Ok(preview)
    }

    pub fn recheck(&self) -> Result<()> {
        clean()?;
        if self.repository != repository_root()?.display().to_string()
            || self.original_head != commit_id("HEAD")?
            || head_branch()?.as_deref() != Some(self.branch.as_str())
            || self.upstream != commit_id(&self.target)?
            || self.index_sha256 != index_sha256()?
        {
            bail!("rebase 分析期间 HEAD、branch、index 或 upstream 已变化");
        }
        Ok(())
    }

    pub fn verify_file(&self, path: &Path) -> Result<()> {
        let saved: Self = read_json(path).context("读取 rebase 计划")?;
        if serde_json::to_vec(&saved)? != serde_json::to_vec(self)? {
            bail!("rebase 计划已过期或分析内容不匹配；请重新执行 --plan");
        }
        Ok(())
    }

    pub fn save(&self, path: &Path, detailed: bool) -> Result<u8> {
        save_new(path, self)?;
        for (index, step) in self.steps.iter().enumerate() {
            eprintln!(
                "rebase 预分析 {}/{}：{}",
                index + 1,
                self.commits.len(),
                self.commits[index]
            );
            repository::report_analysis(&step.analysis.report, detailed);
            if step.native_conflicted {
                eprintln!("Git 预演存在冲突");
            }
        }
        if !self.pending.is_empty() {
            eprintln!(
                "后续 {} 个 commit 尚未分析；需先解决当前冲突，再根据实际结果逐步分析",
                self.pending.len()
            );
        }
        eprintln!("重放计划：{}", path.display());
        Ok(u8::from(
            self.steps.iter().any(|step| step.result_tree.is_none()),
        ))
    }
}
