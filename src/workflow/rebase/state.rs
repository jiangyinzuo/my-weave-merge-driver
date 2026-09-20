//! Persistent wrapper state and report lifecycle; Git owns sequencer state.
use super::super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub git_dir: PathBuf,
    pub directory: PathBuf,
    pub executable: String,
    pub editor: Option<String>,
    pub detailed: bool,
}

pub(super) fn state_path(directory: &Path) -> PathBuf {
    directory.join("strict-weave/rebase.json")
}

pub(super) fn load() -> Result<State> {
    let directory = git_dir()?;
    let state: State = serde_json::from_slice(
        &fs::read(state_path(&directory))
            .context("没有 strict-weave rebase 状态；请使用原生 Git 处理非本工具启动的 rebase")?,
    )?;
    if state.git_dir != directory || !state.directory.starts_with(directory.join("strict-weave")) {
        bail!("rebase 状态与当前仓库不匹配");
    }
    Ok(state)
}

impl State {
    pub fn save(&self) -> Result<()> {
        repository::atomic_write(
            &state_path(&self.git_dir),
            &serde_json::to_vec_pretty(self)?,
        )
    }

    pub fn blocked_path(&self) -> PathBuf {
        self.directory.join("blocked")
    }

    pub fn activate(&self, report: &Path) -> Result<()> {
        let active = self.directory.join("active.json");
        repository::atomic_write(&active, &fs::read(report)?)?;
        let mut permissions = fs::metadata(&active)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(active, permissions)?;
        fs::remove_file(self.blocked_path())?;
        Ok(())
    }

    /// A failed spawn/editor must not leave wrapper state without native state.
    /// Reports survive completion and failure for inspection.
    pub fn finish(&self, result: Result<u8>) -> Result<u8> {
        if !self.git_dir.join("rebase-merge").exists()
            && !self.git_dir.join("rebase-apply").exists()
        {
            fs::remove_file(state_path(&self.git_dir))?;
        }
        result
    }
}
