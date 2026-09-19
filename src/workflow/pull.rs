use super::*;
use clap::{Args, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
pub enum RebaseMode {
    #[value(name = "true")]
    Linear,
    #[value(name = "false")]
    Merge,
    Interactive,
    Merges,
}

#[derive(Args)]
pub struct PullArgs {
    pub remote: Option<String>,
    /// 一个远端 branch/ref；不接受多个 refspec 或目标 ref 更新语法
    pub branch: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true, conflicts_with = "no_rebase")]
    pub rebase: Option<RebaseMode>,
    #[arg(long)]
    pub no_rebase: bool,
    #[arg(long, conflicts_with = "no_ff")]
    pub ff_only: bool,
    #[arg(long)]
    pub no_ff: bool,
    #[arg(long)]
    pub squash: bool,
    #[arg(long)]
    pub no_commit: bool,
    #[arg(long)]
    pub explain_reasons: bool,
}

pub fn run(args: PullArgs) -> Result<u8> {
    idle(&git_dir()?)?;
    clean()?;
    let ours = commit("HEAD")?;
    let local = read(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let remote = match args.remote {
        Some(remote) => remote,
        None => read(&["config", "--get", &format!("branch.{local}.remote")])?,
    };
    let branch = match args.branch {
        Some(branch) => branch,
        None => read(&["config", "--get", &format!("branch.{local}.merge")])?,
    };
    if branch.contains([':', '*', '\n']) || branch.starts_with('-') {
        bail!("pull 只支持单个远端 branch/ref");
    }
    let mode = if args.no_rebase {
        RebaseMode::Merge
    } else if let Some(mode) = args.rebase {
        mode
    } else {
        let value = read(&["config", "--get", &format!("branch.{local}.rebase")])
            .or_else(|_| read(&["config", "--get", "pull.rebase"]))
            .unwrap_or_else(|_| "false".into());
        match value.as_str() {
            "true" => RebaseMode::Linear,
            "false" => RebaseMode::Merge,
            "interactive" => RebaseMode::Interactive,
            "merges" => RebaseMode::Merges,
            _ => bail!("不支持的 pull.rebase 配置：{value}"),
        }
    };
    if !matches!(mode, RebaseMode::Merge) && (args.no_ff || args.squash || args.no_commit) {
        bail!("rebase 模式不能使用 merge 专用选项");
    }
    if args.squash && args.no_ff {
        bail!("--squash 与 --no-ff 不能组合");
    }
    let code = status(git().args([
        "fetch",
        "--no-tags",
        "--no-recurse-submodules",
        "--",
        &remote,
        &branch,
    ]))?;
    if code != 0 {
        return Ok(code);
    }
    let theirs = commit("FETCH_HEAD")?;
    same_head(&ours)?;
    if args.ff_only {
        return merge::run(MergeArgs {
            target: Some(theirs),
            ff: false,
            no_ff: false,
            ff_only: true,
            squash: args.squash,
            no_commit: args.no_commit,
            commit: false,
            edit: false,
            no_edit: true,
            message: None,
            explain_reasons: args.explain_reasons,
            continue_merge: false,
            abort: false,
            quit: false,
        });
    }
    match mode {
        RebaseMode::Merge => merge::run(MergeArgs {
            target: Some(theirs),
            ff: false,
            no_ff: args.no_ff,
            ff_only: false,
            squash: args.squash,
            no_commit: args.no_commit,
            commit: false,
            edit: false,
            no_edit: true,
            message: None,
            explain_reasons: args.explain_reasons,
            continue_merge: false,
            abort: false,
            quit: false,
        }),
        _ => rebase::run(RebaseArgs {
            upstream: Some(theirs),
            branch: None,
            onto: None,
            root: false,
            interactive: matches!(mode, RebaseMode::Interactive),
            rebase_merges: matches!(mode, RebaseMode::Merges),
            explain_reasons: args.explain_reasons,
            continue_rebase: false,
            abort: false,
            skip: false,
            quit: false,
            edit_todo: false,
            show_current_patch: false,
        }),
    }
}
