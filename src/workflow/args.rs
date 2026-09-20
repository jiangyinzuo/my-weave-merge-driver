//! CLI decoding only. Convert flags into validated operation requests before acquiring a lock.
use super::{merge, pull, rebase, stash};
use anyhow::{bail, Result};
use clap::{ArgGroup, Args, Subcommand, ValueEnum};

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

#[derive(Args)]
#[command(group(ArgGroup::new("action").args(["continue_rebase", "abort", "skip", "quit", "edit_todo", "show_current_patch"])))]
pub struct RebaseArgs {
    pub upstream: Option<String>,
    /// 可选的本地分支；省略时使用当前 HEAD
    pub branch: Option<String>,
    #[arg(long)]
    pub onto: Option<String>,
    #[arg(long, conflicts_with = "upstream")]
    pub root: bool,
    #[arg(short, long)]
    pub interactive: bool,
    #[arg(short = 'r', long)]
    pub rebase_merges: bool,
    #[arg(long)]
    pub explain_reasons: bool,
    #[arg(long = "continue")]
    pub continue_rebase: bool,
    #[arg(long)]
    pub abort: bool,
    #[arg(long)]
    pub skip: bool,
    #[arg(long)]
    pub quit: bool,
    #[arg(long)]
    pub edit_todo: bool,
    #[arg(long)]
    pub show_current_patch: bool,
}

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

#[derive(Args)]
pub struct StashArgs {
    #[command(subcommand)]
    pub action: StashAction,
}

#[derive(Subcommand)]
pub enum StashAction {
    Apply(StashOptions),
    Pop(StashOptions),
}

#[derive(Args)]
pub struct StashOptions {
    #[arg(default_value = "stash@{0}")]
    pub stash: String,
    #[arg(long)]
    pub index: bool,
    #[arg(long)]
    pub explain_reasons: bool,
}

/// Also validate direct library callers, which do not pass through Clap.
fn exclusive(flags: &[bool], description: &str) -> Result<()> {
    if flags.iter().filter(|flag| **flag).count() > 1 {
        bail!("互斥参数：{description}");
    }
    Ok(())
}

impl TryFrom<MergeArgs> for merge::Request {
    type Error = anyhow::Error;
    fn try_from(args: MergeArgs) -> Result<Self> {
        exclusive(
            &[args.ff, args.no_ff, args.ff_only],
            "--ff / --no-ff / --ff-only",
        )?;
        exclusive(&[args.no_commit, args.commit], "--no-commit / --commit")?;
        exclusive(&[args.edit, args.no_edit], "--edit / --no-edit")?;
        exclusive(
            &[args.continue_merge, args.abort, args.quit],
            "--continue / --abort / --quit",
        )?;
        if args.squash && (args.no_ff || args.commit) {
            bail!("--squash 不能与 --no-ff / --commit 组合");
        }
        let action = if args.abort {
            Some(merge::Action::Abort)
        } else if args.quit {
            Some(merge::Action::Quit)
        } else if args.continue_merge {
            Some(merge::Action::Continue)
        } else {
            None
        };
        if let Some(action) = action {
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
            return Ok(Self::Resume(action));
        }
        Ok(Self::Start(merge::Options {
            target: args.target,
            ff: if args.ff_only {
                merge::FastForward::Only
            } else if args.no_ff {
                merge::FastForward::Never
            } else {
                merge::FastForward::Allow
            },
            finish: if args.squash {
                merge::Finish::Squash
            } else if args.no_commit {
                merge::Finish::NoCommit
            } else {
                merge::Finish::Commit
            },
            edit: args.edit,
            message: args.message,
            detailed: args.explain_reasons,
        }))
    }
}

impl TryFrom<RebaseArgs> for rebase::Request {
    type Error = anyhow::Error;
    fn try_from(args: RebaseArgs) -> Result<Self> {
        let actions = [
            (args.continue_rebase, rebase::Action::Continue),
            (args.abort, rebase::Action::Abort),
            (args.skip, rebase::Action::Skip),
            (args.quit, rebase::Action::Quit),
            (args.edit_todo, rebase::Action::EditTodo),
            (args.show_current_patch, rebase::Action::ShowCurrentPatch),
        ];
        exclusive(&actions.map(|(enabled, _)| enabled), "rebase 操作")?;
        if let Some((_, action)) = actions.into_iter().find(|(enabled, _)| *enabled) {
            if args.upstream.is_some()
                || args.branch.is_some()
                || args.onto.is_some()
                || args.root
                || args.interactive
                || args.rebase_merges
            {
                bail!("rebase 继续/中止不能同时指定新操作参数");
            }
            return Ok(Self::Resume(action));
        }
        if args.root && args.upstream.is_some() {
            bail!("--root 不能同时指定 upstream");
        }
        Ok(Self::Start(rebase::Options {
            upstream: if args.root {
                rebase::Upstream::Root
            } else {
                rebase::Upstream::Revision(args.upstream)
            },
            branch: args.branch,
            onto: args.onto,
            interactive: args.interactive,
            rebase_merges: args.rebase_merges,
            detailed: args.explain_reasons,
        }))
    }
}

impl TryFrom<PullArgs> for pull::Options {
    type Error = anyhow::Error;
    fn try_from(args: PullArgs) -> Result<Self> {
        exclusive(&[args.ff_only, args.no_ff], "--ff-only / --no-ff")?;
        if args.rebase.is_some() && args.no_rebase {
            bail!("--rebase 与 --no-rebase 不能组合");
        }
        if args.squash && args.no_ff {
            bail!("--squash 与 --no-ff 不能组合");
        }
        Ok(Self {
            remote: args.remote,
            branch: args.branch,
            mode: if args.no_rebase {
                Some(pull::Mode::Merge)
            } else {
                args.rebase.map(Into::into)
            },
            ff: if args.ff_only {
                merge::FastForward::Only
            } else if args.no_ff {
                merge::FastForward::Never
            } else {
                merge::FastForward::Allow
            },
            finish: if args.squash {
                merge::Finish::Squash
            } else if args.no_commit {
                merge::Finish::NoCommit
            } else {
                merge::Finish::Commit
            },
            detailed: args.explain_reasons,
        })
    }
}

impl From<RebaseMode> for pull::Mode {
    fn from(mode: RebaseMode) -> Self {
        match mode {
            RebaseMode::Merge => Self::Merge,
            RebaseMode::Linear => Self::Rebase,
            RebaseMode::Interactive => Self::Interactive,
            RebaseMode::Merges => Self::RebaseMerges,
        }
    }
}

impl TryFrom<StashArgs> for stash::Options {
    type Error = anyhow::Error;
    fn try_from(args: StashArgs) -> Result<Self> {
        let (options, action) = match args.action {
            StashAction::Apply(options) => (options, stash::Action::Apply),
            StashAction::Pop(options) => (options, stash::Action::Pop),
        };
        if action == stash::Action::Pop
            && !options
                .stash
                .strip_prefix("stash@{")
                .and_then(|s| s.strip_suffix('}'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            bail!("stash pop 请使用 stash@{{n}}，以便固定并校验将删除的条目");
        }
        Ok(Self {
            stash: options.stash,
            index: options.index,
            detailed: options.explain_reasons,
            action,
        })
    }
}
