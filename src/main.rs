use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use strict_weave::{
    analysis,
    merge::{self, Labels},
    repository,
};

#[derive(Parser)]
#[command(version, about = "严格 entity + 行级 Git merge driver")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Git merge driver；Git 负责 merge/rebase/cherry-pick/stash 的流程控制
    Driver(Box<Driver>),
    /// 只读生成一次操作所需的全局三方分析报告
    Prepare(PrepareArgs),
}

#[derive(Args)]
struct PrepareArgs {
    /// 明确的 base commit/tree
    base: String,
    /// 明确的 ours commit/tree
    ours: String,
    /// 明确的 theirs commit/tree
    theirs: String,
    /// 新建分析报告，不覆盖已有文件
    #[arg(short = 'o', long = "output")]
    output: PathBuf,
    /// 展开全部分析依据
    #[arg(long)]
    explain_reasons: bool,
}

#[derive(Args)]
#[command(subcommand_negates_reqs = true, args_conflicts_with_subcommands = true)]
struct Driver {
    #[arg(required = true)]
    base: Option<PathBuf>,
    #[arg(required = true)]
    ours: Option<PathBuf>,
    #[arg(required = true)]
    theirs: Option<PathBuf>,
    #[arg(required = true)]
    path: Option<String>,
    #[arg(default_value_t = 7)]
    marker_size: usize,
    #[arg(long, default_value = "来源未识别")]
    ours_label: String,
    #[arg(long, default_value = "来源未识别")]
    base_label: String,
    #[arg(long, default_value = "来源未识别")]
    theirs_label: String,
    /// 只读分析文件；也可用 STRICT_WEAVE_ANALYSIS 指定
    #[arg(long)]
    analysis: Option<PathBuf>,
    /// 可选的紧凑行级展示；严格实体冲突仍保留完整三方范围
    #[arg(long)]
    zdiff3: bool,
    /// 展开父原因下的全部检查依据
    #[arg(long)]
    explain_reasons: bool,
}

fn run() -> Result<u8> {
    let driver = match Cli::parse().command {
        Commands::Driver(driver) => *driver,
        Commands::Prepare(args) => {
            let report = analysis::prepare([&args.base, &args.ours, &args.theirs])?;
            analysis::save(&args.output, &report)?;
            repository::report_analysis(&report, args.explain_reasons);
            eprintln!("分析结果：{}", args.output.display());
            return Ok(u8::from(report.conflicted()));
        }
    };
    let ours = driver.ours.context("缺少 ours")?;
    let path = driver.path.context("缺少 path")?;
    let inputs = [
        std::fs::read(driver.base.context("缺少 base")?).context("读取 base")?,
        std::fs::read(&ours).context("读取 ours")?,
        std::fs::read(driver.theirs.context("缺少 theirs")?).context("读取 theirs")?,
    ];
    let labels = Labels {
        ours: driver.ours_label,
        base: driver.base_label,
        theirs: driver.theirs_label,
    };
    let texts = [
        merge::validate_text(&inputs[0])?,
        merge::validate_text(&inputs[1])?,
        merge::validate_text(&inputs[2])?,
    ];
    let global = driver
        .analysis
        .or_else(|| std::env::var_os("STRICT_WEAVE_ANALYSIS").map(PathBuf::from))
        .map(|file| analysis::load_for_driver(&file, &path, texts))
        .transpose()?;
    let mut outcome = merge::merge_with_style(
        texts[0],
        texts[1],
        texts[2],
        &path,
        &labels,
        driver.marker_size,
        if driver.zdiff3 {
            merge::ConflictStyle::Zdiff3
        } else {
            merge::ConflictStyle::Diff3
        },
    )?;
    if let Some(global) = global {
        analysis::augment(&mut outcome, &global, texts, &labels, driver.marker_size);
    }
    repository::atomic_write(&ours, outcome.content.as_bytes())?;
    repository::report(&path, &outcome, &labels, driver.explain_reasons);
    Ok(u8::from(outcome.conflicted()))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("strict-weave：{error:#}");
            // Git treats >128 as a driver failure, not a successful resolution.
            std::process::ExitCode::from(129)
        }
    }
}
