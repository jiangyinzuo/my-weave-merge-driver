use super::*;
use clap::{Args, Subcommand};
use std::io::Write;

#[derive(Args)]
pub struct StashArgs {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    Apply(Options),
    Pop(Options),
}

#[derive(Args)]
pub struct Options {
    #[arg(default_value = "stash@{0}")]
    pub stash: String,
    #[arg(long)]
    pub index: bool,
    #[arg(long)]
    pub explain_reasons: bool,
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

pub fn run(args: StashArgs) -> Result<u8> {
    let (options, pop) = match args.action {
        Action::Apply(o) => (o, false),
        Action::Pop(o) => (o, true),
    };
    let directory = git_dir()?;
    idle(&directory)?;
    let workspace = workspace(&directory)?;
    let _lock = Lock::acquire(&workspace)?;
    if !read(&["ls-files", "--others", "--exclude-standard", "--", ":/"])?.is_empty() {
        bail!("stash 预分析暂不支持当前工作区有未跟踪文件；请先自行保存或纳入 index");
    }
    if pop
        && !(options.stash.starts_with("stash@{")
            && options.stash.len() > 8
            && options.stash.ends_with('}')
            && options.stash[7..options.stash.len() - 1]
                .bytes()
                .all(|b| b.is_ascii_digit()))
    {
        bail!("stash pop 请使用 stash@{{n}}，以便固定并校验将删除的条目");
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
    let theirs = stash_tree(&stash, parents.get(2).copied(), &workspace)?;
    let (_, work_conflict) = prepare(
        &workspace,
        [parents[0], &worktree, &theirs],
        options.explain_reasons,
    )?;
    let (report, index_conflict) = prepare(
        &workspace,
        [parents[0], &index, &theirs],
        options.explain_reasons,
    )?;
    let restore_conflict = if options.index {
        prepare(
            &workspace,
            [parents[0], &index, parents[1]],
            options.explain_reasons,
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
