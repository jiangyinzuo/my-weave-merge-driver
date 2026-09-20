use super::*;
use std::io::Write;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Apply,
    Pop,
}

pub(super) struct Options {
    pub stash: String,
    pub index: bool,
    pub detailed: bool,
    pub action: Action,
}

fn tracked_worktree(head: &str) -> Result<String> {
    let snapshot = read(&["stash", "create"])?;
    read(&[
        "rev-parse",
        &format!(
            "{}^{{tree}}",
            if snapshot.is_empty() { head } else { &snapshot }
        ),
    ])
}

/// Stash's third parent stores untracked files separately. Include them in
/// candidate analysis without touching the real index or working tree.
fn stash_tree(stash: &str, untracked: Option<&str>, workspace: &Path) -> Result<String> {
    let Some(untracked) = untracked else {
        return read(&["rev-parse", &format!("{stash}^{{tree}}")]);
    };
    let scratch = tempfile::tempdir_in(workspace)?;
    let index = scratch.path().join("index");
    let mut command = git();
    command
        .env("GIT_INDEX_FILE", &index)
        .args(["read-tree", stash]);
    if !command.status()?.success() {
        bail!("无法读取 stash tree");
    }
    let entries = git()
        .args(["ls-tree", "--full-tree", "-rz", untracked])
        .output()?;
    if !entries.status.success() {
        bail!("无法读取 stash 未跟踪文件");
    }
    let mut child = git()
        .env("GIT_INDEX_FILE", &index)
        .args(["update-index", "-z", "--index-info"])
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("无法打开 index 输入")?
        .write_all(&entries.stdout)?;
    if !child.wait()?.success() {
        bail!("无法组合 stash tree");
    }
    let result = git()
        .env("GIT_INDEX_FILE", index)
        .arg("write-tree")
        .output()?;
    if !result.status.success() {
        bail!("无法写出 stash 分析 tree");
    }
    Ok(String::from_utf8(result.stdout)?.trim().into())
}

pub(super) fn run(operation: &Operation, options: Options) -> Result<u8> {
    idle(&operation.git_dir)?;
    let workspace = &operation.workspace;
    let pop = options.action == Action::Pop;
    if !read(&["ls-files", "--others", "--exclude-standard", "--", ":/"])?.is_empty() {
        bail!("stash 预分析暂不支持当前工作区有未跟踪文件；请先自行保存或纳入 index");
    }
    let stash = commit(&options.stash)?;
    let parents = read(&["rev-list", "--parents", "-n", "1", &stash])?;
    let parents: Vec<_> = parents.split_whitespace().skip(1).collect();
    if !(2..=3).contains(&parents.len()) {
        bail!("目标不是支持的 stash commit");
    }
    let head = commit("HEAD")?;
    let index = read(&["write-tree"])?;
    let worktree = tracked_worktree(&head)?;
    let theirs = stash_tree(&stash, parents.get(2).copied(), workspace)?;
    let (_, work_conflict) = prepare(
        workspace,
        [parents[0], &worktree, &theirs],
        options.detailed,
    )?;
    let (report, index_conflict) =
        prepare(workspace, [parents[0], &index, &theirs], options.detailed)?;
    let restore_conflict = if options.index {
        prepare(
            workspace,
            [parents[0], &index, parents[1]],
            options.detailed,
        )?
        .1
    } else {
        false
    };
    if work_conflict || index_conflict || restore_conflict {
        eprintln!("预分析发现审核项，未应用或删除 stash");
        return Ok(1);
    }
    if commit("HEAD")? != head
        || read(&["write-tree"])? != index
        || tracked_worktree(&head)? != worktree
        || (pop && commit(&options.stash)? != stash)
    {
        bail!("stash 或工作区在预分析期间变化，未执行操作");
    }
    let mut command = operation_git()?;
    command.args(["stash", if pop { "pop" } else { "apply" }]);
    if options.index {
        command.arg("--index");
    }
    command
        .arg(if pop { &options.stash } else { &stash })
        .env("STRICT_WEAVE_ANALYSIS", report);
    status(&mut command)
}
