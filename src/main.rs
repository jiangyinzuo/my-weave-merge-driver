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
    Merge(MergeArgs),
    /// 重放一个非 merge commit
    CherryPick(MergeArgs),
}

#[derive(Args)]
struct MergeArgs {
    /// 唯一的 merge target；当前阶段不接受额外 Git 选项
    target: String,
    #[arg(long)]
    zdiff3: bool,
    #[arg(long)]
    explain_reasons: bool,
}

impl MergeArgs {
    fn options(&self) -> operation::Options {
        operation::Options {
            zdiff3: self.zdiff3,
            detailed: self.explain_reasons,
        }
    }
}

fn run() -> Result<u8> {
    match Cli::parse().command {
        Commands::Merge(args) => operation::merge(&args.target, args.options()),
        Commands::CherryPick(args) => operation::cherry_pick(&args.target, args.options()),
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
