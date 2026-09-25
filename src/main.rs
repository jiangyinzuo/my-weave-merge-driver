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
    /// 重放当前 branch 的一个 commit 到 upstream
    Rebase(OperationArgs),
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
    #[arg(long, conflicts_with = "apply")]
    plan: bool,
    #[arg(short = 'o', long, requires = "plan")]
    output: Option<std::path::PathBuf>,
    #[arg(long, conflicts_with_all = ["plan", "output"])]
    apply: Option<std::path::PathBuf>,
    #[arg(long)]
    zdiff3: bool,
    #[arg(long)]
    explain_reasons: bool,
}

#[derive(Args)]
struct OperationArgs {
    /// 唯一的 merge target；当前阶段不接受额外 Git 选项
    target: String,
    #[arg(long, conflicts_with = "apply")]
    plan: bool,
    #[arg(short = 'o', long, requires = "plan")]
    output: Option<std::path::PathBuf>,
    #[arg(long, conflicts_with_all = ["plan", "output"])]
    apply: Option<std::path::PathBuf>,
    #[arg(long)]
    zdiff3: bool,
    #[arg(long)]
    explain_reasons: bool,
}

impl OperationArgs {
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

impl StashArgs {
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
        Commands::Rebase(args) => operation::rebase(&args.target, args.options(), args.mode()?),
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
