use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use strict_weave::operation;

#[derive(Parser)]
#[command(version, about = "基于 Git plumbing 的严格 entity + 行级合并工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 分析并执行单目标 merge；冲突后保留标准 Git 状态
    Merge(OperationArgs),
    /// 重放一个非 merge commit
    CherryPick(OperationArgs),
    /// 将当前 branch 的普通 commit 重放到 upstream
    Rebase(RebaseArgs),
    /// 应用一个普通 stash
    Stash(StashCommand),
}

#[derive(Args)]
struct StashCommand {
    #[command(subcommand)]
    action: StashAction,
}

#[derive(Subcommand)]
enum StashAction {
    /// 应用并在无冲突时删除 stash
    Pop(StashArgs),
    /// 应用但保留 stash
    Apply(StashArgs),
}

#[derive(Args)]
struct StashArgs {
    #[arg(default_value = "stash@{0}")]
    target: String,
    #[command(flatten)]
    options: OperationOptions,
}

#[derive(Args)]
struct OperationArgs {
    /// 要合并、重放或作为 rebase upstream 的 Git revision
    target: String,
    #[command(flatten)]
    options: OperationOptions,
}

#[derive(Args)]
struct RebaseArgs {
    /// 要重放到的 upstream；继续或中止时省略
    #[arg(value_name = "UPSTREAM", conflicts_with_all = ["continue_", "abort"])]
    target: Option<String>,
    #[arg(long = "continue", conflicts_with_all = ["target", "abort", "plan", "output", "apply"])]
    continue_: bool,
    #[arg(long, conflicts_with_all = ["target", "continue_", "plan", "output", "apply"])]
    abort: bool,
    #[command(flatten)]
    options: OperationOptions,
}

#[derive(Args)]
struct OperationOptions {
    #[arg(long, conflicts_with = "apply")]
    /// 只分析并写出计划，不修改 HEAD、index 或工作区
    plan: bool,
    #[arg(short = 'o', long, requires = "plan", value_name = "FILE")]
    /// `--plan` 的输出文件
    output: Option<std::path::PathBuf>,
    #[arg(long, conflicts_with_all = ["plan", "output"], value_name = "FILE")]
    /// 校验并应用已有计划
    apply: Option<std::path::PathBuf>,
    #[arg(long, help = "使用 Git zdiff3 展示冲突块")]
    zdiff3: bool,
    #[arg(long, help = "输出完整的分析依据")]
    explain_reasons: bool,
}

impl OperationArgs {
    fn mode(&self) -> anyhow::Result<operation::Mode> {
        self.options.mode()
    }
    fn options(&self) -> operation::Options {
        self.options.options()
    }
}

impl RebaseArgs {
    fn options(&self) -> operation::Options {
        self.options.options()
    }
}

impl StashArgs {
    fn mode(&self) -> anyhow::Result<operation::Mode> {
        self.options.mode()
    }
    fn options(&self) -> operation::Options {
        self.options.options()
    }
}

impl OperationOptions {
    fn mode(&self) -> anyhow::Result<operation::Mode> {
        if let Some(path) = &self.apply {
            return Ok(operation::Mode::ApplyPlan(path.clone()));
        }
        if self.plan {
            return Ok(operation::Mode::Plan(
                self.output
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("--plan 需要 --output"))?,
            ));
        }
        Ok(operation::Mode::Apply)
    }
    fn options(&self) -> operation::Options {
        operation::Options {
            zdiff3: self.zdiff3,
            detailed: self.explain_reasons,
        }
    }
}

fn run() -> Result<u8> {
    match Cli::parse().command {
        Commands::Merge(args) => operation::merge(&args.target, args.options(), args.mode()?),
        Commands::CherryPick(args) => {
            operation::cherry_pick(&args.target, args.options(), args.mode()?)
        }
        Commands::Rebase(args) => {
            if args.continue_ {
                operation::rebase_continue(args.options())
            } else if args.abort {
                operation::rebase_abort()
            } else {
                let target = args.target.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("rebase 需要 UPSTREAM、--continue 或 --abort")
                })?;
                operation::rebase(target, args.options(), args.options.mode()?)
            }
        }
        Commands::Stash(command) => match command.action {
            StashAction::Pop(args) => {
                operation::stash(&args.target, true, args.options(), args.mode()?)
            }
            StashAction::Apply(args) => {
                operation::stash(&args.target, false, args.options(), args.mode()?)
            }
        },
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("strict-weave：{error:#}");
            std::process::ExitCode::from(129)
        }
    }
}
