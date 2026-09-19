use super::*;
use clap::{ArgGroup, Args};

#[derive(Args)]
#[command(group(ArgGroup::new("ff_mode").args(["ff", "no_ff", "ff_only"])))]
#[command(group(ArgGroup::new("commit_mode").args(["no_commit", "commit"])))]
#[command(group(ArgGroup::new("action").args(["continue_merge", "abort", "quit"])))]
pub struct MergeArgs {
    /// 一个要合入的 commit/branch；省略时使用当前分支的 upstream
    pub target: Option<String>,
    #[arg(long)]
    pub ff: bool,
    #[arg(long)]
    pub no_ff: bool,
    #[arg(long)]
    pub ff_only: bool,
    #[arg(long, conflicts_with_all = ["no_ff", "commit"])]
    pub squash: bool,
    #[arg(long)]
    pub no_commit: bool,
    #[arg(long)]
    pub commit: bool,
    #[arg(long, conflicts_with = "no_edit")]
    pub edit: bool,
    #[arg(long)]
    pub no_edit: bool,
    #[arg(short, long)]
    pub message: Option<String>,
    #[arg(long)]
    pub explain_reasons: bool,
    #[arg(long = "continue")]
    pub continue_merge: bool,
    #[arg(long)]
    pub abort: bool,
    #[arg(long)]
    pub quit: bool,
}

pub fn run(args: MergeArgs) -> Result<u8> {
    let directory = git_dir()?;
    let workspace = workspace(&directory)?;
    let _lock = Lock::acquire(&workspace)?;
    if args.continue_merge || args.abort || args.quit {
        if args.target.is_some()
            || args.ff
            || args.no_ff
            || args.ff_only
            || args.squash
            || args.no_commit
            || args.commit
            || args.edit
            || args.no_edit
            || args.message.is_some()
        {
            bail!("继续/中止操作不能同时指定新的 merge 参数");
        }
        let flag = if args.abort {
            "--abort"
        } else if args.quit {
            "--quit"
        } else {
            "--continue"
        };
        return status(git().args(["merge", flag]));
    }
    idle(&directory)?;
    clean()?;
    let ours = commit("HEAD")?;
    let theirs = commit(args.target.as_deref().unwrap_or("@{upstream}"))?;
    let bases = read(&["merge-base", "--all", &ours, &theirs])?;
    let bases: Vec<_> = bases.lines().collect();
    if bases.len() != 1 {
        bail!("需要唯一 merge base；多 base/虚拟 base 和无共同历史暂不支持");
    }
    let (report, conflicted) =
        prepare(&workspace, [bases[0], &ours, &theirs], args.explain_reasons)?;
    if conflicted {
        eprintln!("预分析发现审核项，未执行 git merge；HEAD、index 和工作区保持不变");
        return Ok(1);
    }
    same_head(&ours)?;
    let mut command = operation_git()?;
    // branch.*.mergeOptions can inject -Xours/-sours or unrelated options.
    // Clear the branch default rather than relying on last-option precedence.
    if let Ok(branch) = read(&["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        command
            .arg("-c")
            .arg(format!("branch.{branch}.mergeOptions="));
    }
    command.args([
        "-c",
        "merge.autoStash=false",
        "merge",
        "--strategy=ort",
        "--no-autostash",
    ]);
    // Explicit flags prevent merge.ff from silently
    // selecting a different operation than the wrapper's advertised mode.
    command.arg(if args.ff_only {
        "--ff-only"
    } else if args.no_ff {
        "--no-ff"
    } else {
        "--ff"
    });
    command.arg(if args.squash {
        "--squash"
    } else {
        "--no-squash"
    });
    if !args.squash {
        command.arg(if args.no_commit {
            "--no-commit"
        } else {
            "--commit"
        });
    }
    command.arg(if args.edit { "--edit" } else { "--no-edit" });
    if let Some(message) = args.message {
        command.arg("--message").arg(message);
    }
    command
        .arg("--")
        .arg(&theirs)
        .env("STRICT_WEAVE_ANALYSIS", report);
    status(&mut command)
}
