//! Optional Git orchestration. Core analysis and the driver remain independent.
//! Reject ambiguous contexts; Git owns commits, index conflicts and rollback.
mod args;
mod merge;
mod pull;
mod rebase;
mod stash;
pub use args::{MergeArgs, PullArgs, RebaseArgs, StashArgs};
pub use rebase::{check as rebase_check, edit_todo as rebase_todo};

use crate::{analysis, repository};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// One lock spans every phase of an operation, including pull's fetch and
/// delegated merge/rebase. Sequencer callbacks run under their parent's lock.
struct Operation {
    git_dir: PathBuf,
    workspace: PathBuf,
    _lock: Lock,
}
impl Operation {
    fn acquire() -> Result<Self> {
        let git_dir = git_dir()?;
        let workspace = workspace(&git_dir)?;
        let lock = Lock::acquire(&workspace)?;
        Ok(Self {
            git_dir,
            workspace,
            _lock: lock,
        })
    }
}

pub fn merge(args: MergeArgs) -> Result<u8> {
    let request = args.try_into()?;
    merge::run(&Operation::acquire()?, request)
}
pub fn rebase(args: RebaseArgs) -> Result<u8> {
    let request = args.try_into()?;
    rebase::run(&Operation::acquire()?, request)
}
pub fn pull(args: PullArgs) -> Result<u8> {
    let options = args.try_into()?;
    pull::run(&Operation::acquire()?, options)
}
pub fn stash(args: StashArgs) -> Result<u8> {
    let options = args.try_into()?;
    stash::run(&Operation::acquire()?, options)
}

/// Never inherit an unrelated analysis report into a new operation.
fn git() -> Command {
    let mut command = Command::new("git");
    command.env_remove("STRICT_WEAVE_ANALYSIS");
    command
}

fn read(args: &[&str]) -> Result<String> {
    let output = git().env("GIT_OPTIONAL_LOCKS", "0").args(args).output()?;
    if !output.status.success() {
        bail!(
            "git {}：{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?
        .trim_end_matches('\n')
        .into())
}

/// Only exit 1 means absent for the supported optional queries (config,
/// symbolic-ref, merge-base). I/O errors and other Git failures propagate.
fn read_optional(args: &[&str]) -> Result<Option<String>> {
    let output = git().env("GIT_OPTIONAL_LOCKS", "0").args(args).output()?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8(output.stdout)?
                .trim_end_matches('\n')
                .into(),
        )),
        Some(1) => Ok(None),
        _ => bail!(
            "git {}：{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn merge_base(ours: &str, theirs: &str) -> Result<String> {
    let bases = read_optional(&["merge-base", "--all", ours, theirs])?.unwrap_or_default();
    let mut lines = bases.lines();
    match (lines.next(), lines.next()) {
        (Some(base), None) => Ok(base.into()),
        _ => bail!("需要唯一 merge base；多 base/虚拟 base 和无共同历史暂不支持"),
    }
}

fn commit(revision: &str) -> Result<String> {
    read(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{revision}^{{commit}}"),
    ])
}

fn git_dir() -> Result<PathBuf> {
    Ok(PathBuf::from(read(&["rev-parse", "--absolute-git-dir"])?))
}

fn clean() -> Result<()> {
    if !read(&["status", "--porcelain", "--untracked-files=normal"])?.is_empty() {
        bail!("需要干净的工作区和 index（包括未跟踪文件）；请先提交或自行保存修改");
    }
    Ok(())
}

fn idle(directory: &Path) -> Result<()> {
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        if directory.join(name).exists() {
            bail!("已有 Git 操作进行中：{name}；先继续或中止该操作");
        }
    }
    Ok(())
}

fn same_head(expected: &str) -> Result<()> {
    if commit("HEAD")? != expected {
        bail!("预分析期间 HEAD 已变化，未执行 Git 操作；请重新运行");
    }
    clean()
}

fn status(command: &mut Command) -> Result<u8> {
    let status = command.status().context("无法启动 Git 操作")?;
    Ok(status
        .code()
        .and_then(|c| u8::try_from(c).ok())
        .unwrap_or(129))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn executable() -> Result<String> {
    std::env::current_exe()?
        .into_os_string()
        .into_string()
        .map_err(|_| anyhow::anyhow!("可执行文件路径必须为 UTF-8"))
}

/// Use the current executable for the named driver, without persisting config.
/// File selection still belongs to .gitattributes; other drivers remain Git's.
fn operation_git() -> Result<Command> {
    let mut command = git();
    command.arg("-c").arg(format!(
        "merge.strict-weave.driver={} driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y",
        shell_quote(&executable()?)
    ));
    command.args([
        "-c",
        "merge.renormalize=false",
        "-c",
        "rerere.enabled=false",
    ]);
    Ok(command)
}

/// Reports are immutable and survive a stopped operation for human inspection.
fn prepare(directory: &Path, revisions: [&str; 3], detailed: bool) -> Result<(PathBuf, bool)> {
    let report = analysis::prepare(revisions)?;
    let folder = tempfile::Builder::new()
        .prefix("report-")
        .tempdir_in(directory)?;
    let path = folder.path().join("analysis.json");
    analysis::save(&path, &report)?;
    let _ = folder.keep();
    repository::report_analysis(&report, "需审核", detailed);
    eprintln!("分析结果：{}", path.display());
    Ok((path, report.conflicted()))
}

fn workspace(directory: &Path) -> Result<PathBuf> {
    let path = directory.join("strict-weave");
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Serialize wrapper invocations, not arbitrary concurrent native Git commands.
struct Lock(PathBuf);
impl Lock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join("workflow.lock");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context(
                "已有 strict-weave 操作；若进程异常退出，请确认无操作运行后删除 workflow.lock",
            )?;
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn empty_tree() -> Result<String> {
    let output = git().arg("mktree").stdin(Stdio::null()).output()?;
    if !output.status.success() {
        bail!("无法创建空 tree");
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
