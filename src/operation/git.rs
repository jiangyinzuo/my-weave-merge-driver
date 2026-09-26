//! Git plumbing and operation guards.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

pub(super) fn git(args: &[&str]) -> Result<Output> {
    Ok(Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()?)
}
pub(super) fn checked(output: Output) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!(
            "Git 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}
pub(super) fn read(args: &[&str]) -> Result<Vec<u8>> {
    checked(git(args)?)
}
pub(super) fn string(args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(read(args)?)?
        .trim_end_matches('\n')
        .into())
}
pub(super) fn input(args: &[&str], bytes: &[u8]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command.args(args);
    input_command(command, bytes)
}
fn input_command(mut command: Command, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("缺少 Git stdin")?
        .write_all(bytes)?;
    checked(child.wait_with_output()?)
}
pub(super) fn commit_id(name: &str) -> Result<String> {
    string(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{name}^{{commit}}"),
    ])
}
pub(super) fn head_branch() -> Result<Option<String>> {
    let output = git(&["symbolic-ref", "-q", "HEAD"])?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    Ok(Some(String::from_utf8(checked(output)?)?.trim_end().into()))
}
pub(super) fn git_path(name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(string(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        name,
    ])?))
}
pub(super) fn repository_root() -> Result<PathBuf> {
    Ok(std::fs::canonicalize(string(&[
        "rev-parse",
        "--show-toplevel",
    ])?)?)
}
pub(super) fn index_sha256() -> Result<String> {
    let path = git_path("index")?;
    let bytes = std::fs::read(path).context("读取 Git index")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub(super) fn unique_base(ours: &str, theirs: &str) -> Result<String> {
    let bases = string(&["merge-base", "--all", ours, theirs])?;
    if bases.lines().count() != 1 {
        bail!("暂不支持无共同祖先或多个 merge-base");
    }
    Ok(bases)
}
pub(super) fn clean() -> Result<()> {
    if !read(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?.is_empty() {
        bail!("工作区或 index 不干净；请先提交或保存当前修改");
    }
    Ok(())
}

pub(super) struct Guard(PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(super) fn acquire() -> Result<Guard> {
    if std::env::var_os("GIT_INDEX_FILE").is_some() {
        bail!("不支持 GIT_INDEX_FILE");
    }
    let root = repository_root()?;
    std::env::set_current_dir(root)?;
    let dir = git_path("strict-weave")?;
    std::fs::create_dir_all(&dir)?;
    let lock = dir.join("operation.lock");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .context("已有 strict-weave 操作；确认无运行进程后再移除 operation.lock")?;
    Ok(Guard(lock))
}

pub(super) fn ensure_no_git_operation(allowed_cherry: Option<&str>) -> Result<()> {
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        if git_path(name)?.exists() {
            if name == "CHERRY_PICK_HEAD" && allowed_cherry == Some(commit_id(name)?.as_str()) {
                continue;
            }
            bail!("已有 Git 操作：{name}；请先由 Git 处理该操作");
        }
    }
    Ok(())
}

pub(super) fn enter() -> Result<Guard> {
    let guard = acquire()?;
    ensure_no_git_operation(None)?;
    if git_path("strict-weave/rebase-state.json")?.exists() {
        bail!("已有 strict-weave rebase；请使用 rebase --continue 或 --abort");
    }
    clean()?;
    Ok(guard)
}

pub(super) fn native_output(args: &[&str]) -> Result<Output> {
    let mut prefix = vec![
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "rerere.enabled=false",
        "-c",
        "merge.renormalize=false",
        "-c",
        "merge.conflictStyle=diff3",
    ];
    prefix.extend(args);
    git(&prefix)
}
pub(super) fn native(args: &[&str]) -> Result<Output> {
    let output = native_output(args)?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    Ok(output)
}

/// Create the replayed commit without moving HEAD. Persisting its OID before
/// update-ref lets recovery distinguish a completed update from a failed one.
pub(super) fn replay_commit(tree: &str, parent: &str, original: &str) -> Result<String> {
    let author = string(&[
        "show",
        "-s",
        "--no-show-signature",
        "--format=%an%x00%ae%x00%aI",
        original,
    ])?;
    let fields: Vec<_> = author.split('\0').collect();
    if fields.len() != 3 {
        bail!("无法读取原 commit 的 author");
    }
    // format: preserves the message verbatim; tformat: adds a record newline.
    let message = read(&[
        "show",
        "-s",
        "--no-show-signature",
        "--format=format:%B",
        original,
    ])?;
    let mut command = Command::new("git");
    command
        .args(["commit-tree", tree, "-p", parent])
        .env("GIT_AUTHOR_NAME", fields[0])
        .env("GIT_AUTHOR_EMAIL", fields[1])
        .env("GIT_AUTHOR_DATE", fields[2]);
    Ok(String::from_utf8(input_command(command, &message)?)?
        .trim_end()
        .into())
}
pub(super) fn finish_commit(args: &[&str]) -> Result<u8> {
    let mut full = vec![
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "rerere.enabled=false",
    ];
    full.extend(args);
    let output = git(&full)?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    if output.status.success() {
        Ok(0)
    } else {
        bail!("提交未完成；保留当前 index 和工作区，请检查 Git 输出")
    }
}
