//! Restricted Git porcelain. Analysis and rendering precede repository writes;
//! Git owns native operation state, and strict-weave installs standard stages.
use crate::{analysis, merge, repository};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const ARTIFACT_VERSION: u32 = 1;
const MAX_ARTIFACT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub zdiff3: bool,
    pub detailed: bool,
}

pub enum Mode {
    Apply,
    Plan(PathBuf),
    ApplyPlan(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    artifact_version: u32,
    operation: String,
    target: String,
    repository: String,
    revisions: [String; 3],
    head: String,
    index_sha256: String,
    report: analysis::Report,
}

fn git(args: &[&str]) -> Result<Output> {
    Ok(Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()?)
}
fn checked(output: Output) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!(
            "Git 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}
fn read(args: &[&str]) -> Result<Vec<u8>> {
    checked(git(args)?)
}
fn string(args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(read(args)?)?
        .trim_end_matches('\n')
        .into())
}
fn input(args: &[&str], bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .args(args)
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
fn commit_id(name: &str) -> Result<String> {
    string(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{name}^{{commit}}"),
    ])
}
fn git_path(name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(string(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        name,
    ])?))
}
fn repository_root() -> Result<PathBuf> {
    Ok(std::fs::canonicalize(string(&[
        "rev-parse",
        "--show-toplevel",
    ])?)?)
}
fn index_sha256() -> Result<String> {
    let path = git_path("index")?;
    let bytes = std::fs::read(path).context("读取 Git index")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn unique_base(ours: &str, theirs: &str) -> Result<String> {
    let bases = string(&["merge-base", "--all", ours, theirs])?;
    if bases.lines().count() != 1 {
        bail!("暂不支持无共同祖先或多个 merge-base");
    }
    Ok(bases)
}
fn clean() -> Result<()> {
    if !read(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?.is_empty() {
        bail!("工作区或 index 不干净；请先提交或保存当前修改");
    }
    Ok(())
}

struct Guard(PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn enter() -> Result<Guard> {
    if std::env::var_os("GIT_INDEX_FILE").is_some() {
        bail!("不支持 GIT_INDEX_FILE");
    }
    let root = repository_root()?;
    std::env::set_current_dir(root)?;
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        if git_path(name)?.exists() {
            bail!("已有 Git 操作：{name}；请先由 Git 处理该操作");
        }
    }
    let dir = git_path("strict-weave")?;
    std::fs::create_dir_all(&dir)?;
    let lock = dir.join("operation.lock");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .context("已有 strict-weave 操作；确认无运行进程后再移除 operation.lock")?;
    let guard = Guard(lock);
    clean()?;
    Ok(guard)
}

fn check_subset(revisions: [&str; 3], paths: &BTreeSet<String>) -> Result<()> {
    for side in &revisions[1..] {
        let status = read(&[
            "diff-tree",
            "-r",
            "--no-commit-id",
            "--name-status",
            "-z",
            "-M",
            revisions[0],
            side,
        ])?;
        let mut fields = status.split(|byte| *byte == 0).filter(|s| !s.is_empty());
        while let Some(kind) = fields.next() {
            if kind.starts_with(b"R") {
                bail!("暂不支持 Git 文件 rename；跨文件 function 移动仍会分析");
            }
            fields.next();
        }
    }
    for path in paths {
        for parent in Path::new(path).ancestors().skip(1) {
            if paths.contains(parent.to_str().context("非 UTF-8 路径")?) {
                bail!("暂不支持文件/目录布局冲突：{path}");
            }
        }
    }
    let names: Vec<u8> = paths.iter().flat_map(|p| p.bytes().chain([0])).collect();
    for source in revisions.into_iter().map(Some).chain([None]) {
        let option = source.map(|s| format!("--source={s}"));
        let mut args = vec!["check-attr", "-z", "--stdin"];
        if let Some(option) = &option {
            args.push(option);
        }
        args.extend([
            "merge",
            "filter",
            "working-tree-encoding",
            "ident",
            "text",
            "eol",
        ]);
        let output = input(&args, &names)?;
        let fields: Vec<_> = output
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .collect();
        for attr in fields.chunks(3) {
            if attr.len() != 3 {
                bail!("Git check-attr 输出不完整");
            }
            let value = attr[2];
            let allowed = value == b"unspecified"
                || (value == b"unset" && attr[1] != b"merge")
                || (attr[1] == b"merge" && (value == b"set" || value == b"text"));
            if !allowed {
                bail!(
                    "暂不支持影响合并原文的 attribute：{} {}={}",
                    String::from_utf8_lossy(attr[0]),
                    String::from_utf8_lossy(attr[1]),
                    String::from_utf8_lossy(value)
                );
            }
        }
    }
    if let Ok(value) = string(&["config", "--get", "core.autocrlf"]) {
        if value != "false" {
            bail!("暂不支持 core.autocrlf；需要原文三方输入");
        }
    }
    if let Ok(value) = string(&["config", "--bool", "core.sparseCheckout"]) {
        if value == "true" {
            bail!("暂不支持 sparse checkout");
        }
    }
    Ok(())
}

struct Plan {
    artifact: Artifact,
    labels: merge::Labels,
    outcomes: BTreeMap<String, merge::Outcome>,
    stages: Vec<u8>,
}
impl Plan {
    fn load_snapshots(revisions: [&str; 3]) -> Result<[(String, analysis::TreeSnapshot); 3]> {
        Ok([
            analysis::snapshot(revisions[0])?,
            analysis::snapshot(revisions[1])?,
            analysis::snapshot(revisions[2])?,
        ])
    }
    fn from_loaded(
        artifact: Artifact,
        label: &str,
        loaded: [(String, analysis::TreeSnapshot); 3],
        options: Options,
    ) -> Result<Self> {
        let revisions = artifact.revisions.each_ref().map(String::as_str);
        let trees = loaded.each_ref().map(|(id, _)| id.clone());
        if artifact.report.trees != trees {
            bail!("分析报告的 tree ID 与当前 Git 对象不匹配");
        }
        let snapshots = loaded.each_ref().map(|(_, snapshot)| snapshot);
        let paths = snapshots
            .iter()
            .flat_map(|snapshot| snapshot.keys().cloned())
            .collect();
        check_subset(revisions, &paths)?;
        artifact.report.validate()?;
        let labels = merge::Labels {
            base: revisions[0].into(),
            ours: revisions[1].into(),
            theirs: label.into(),
        };
        let style = if options.zdiff3 {
            merge::ConflictStyle::Zdiff3
        } else {
            merge::ConflictStyle::Diff3
        };
        let mut outcomes = BTreeMap::new();
        let mut stages = Vec::new();
        for (path, global) in &artifact.report.files {
            let texts = snapshots
                .each_ref()
                .map(|snapshot| snapshot.get(path).map(String::as_str).unwrap_or(""));
            let mut outcome =
                merge::merge_with_style(texts[0], texts[1], texts[2], path, &labels, 7, style)?;
            analysis::apply_file_analysis(&mut outcome, global, texts, &labels, 7);
            if outcome.conflicted() {
                stages.extend_from_slice(
                    format!("0 {}\t{path}\0", "0".repeat(revisions[1].len())).as_bytes(),
                );
                for (stage, revision) in revisions.iter().enumerate() {
                    if let Some((mode, oid)) = analysis::blob_oid(revision, path)? {
                        stages.extend_from_slice(
                            format!("{mode} {oid} {}\t{path}\0", stage + 1).as_bytes(),
                        );
                    }
                }
                outcomes.insert(path.clone(), outcome);
            }
        }
        Ok(Self {
            artifact,
            labels,
            outcomes,
            stages,
        })
    }
    fn build(
        operation: &str,
        target: &str,
        revisions: [String; 3],
        options: Options,
    ) -> Result<Self> {
        let refs = revisions.each_ref().map(String::as_str);
        let loaded = Self::load_snapshots(refs)?;
        let trees = loaded.each_ref().map(|(id, _)| id.clone());
        let snapshots = loaded.each_ref().map(|(_, snapshot)| snapshot);
        let paths = snapshots
            .iter()
            .flat_map(|snapshot| snapshot.keys().cloned())
            .collect();
        check_subset(refs, &paths)?;
        let report = analysis::analyze(
            &[
                snapshots[0].clone(),
                snapshots[1].clone(),
                snapshots[2].clone(),
            ],
            trees,
        )?;
        let artifact = Artifact {
            artifact_version: ARTIFACT_VERSION,
            operation: operation.into(),
            target: target.into(),
            repository: repository_root()?.display().to_string(),
            revisions,
            head: commit_id("HEAD")?,
            index_sha256: index_sha256()?,
            report,
        };
        Self::from_loaded(artifact, target, loaded, options)
    }
    fn save_new(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.artifact)?;
        if bytes.len() as u64 > MAX_ARTIFACT {
            bail!("分析计划超过 64 MiB");
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path)
            .map_err(|error| error.error)
            .context("计划文件已存在；不会覆盖旧计划")?;
        Ok(())
    }
    fn report(&self, detailed: bool) {
        repository::report_analysis(&self.artifact.report, detailed);
    }
    fn recheck(&self) -> Result<()> {
        clean()?;
        if commit_id("HEAD")? != self.artifact.revisions[1] {
            bail!("分析期间 HEAD 已变化，拒绝执行");
        }
        if index_sha256()? != self.artifact.index_sha256 {
            bail!("分析期间 Git index 已变化，拒绝执行");
        }
        if repository_root()?.display().to_string() != self.artifact.repository {
            bail!("分析计划属于另一个仓库");
        }
        if commit_id(&self.artifact.target)? != self.artifact.revisions[2] {
            bail!("分析计划的 target 已变化，拒绝执行");
        }
        Ok(())
    }
    fn install(&self, options: Options) -> Result<bool> {
        input(&["update-index", "-z", "--index-info"], &self.stages)?;
        for (path, outcome) in &self.outcomes {
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            repository::atomic_write(Path::new(path), outcome.content.as_bytes())?;
            repository::report(path, outcome, &self.labels, options.detailed);
        }
        Ok(!read(&["ls-files", "-u", "-z"])?.is_empty())
    }
}

fn load_artifact(
    path: &Path,
    operation: &str,
    target: &str,
    revisions: [String; 3],
    options: Options,
) -> Result<Plan> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .context("读取分析计划")?
        .take(MAX_ARTIFACT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("分析计划超过 64 MiB");
    }
    let artifact: Artifact = serde_json::from_slice(&bytes).context("无效的分析计划")?;
    if artifact.artifact_version != ARTIFACT_VERSION
        || artifact.operation != operation
        || artifact.target != target
        || artifact.revisions != revisions
    {
        bail!("分析计划与当前操作不匹配；请重新执行 --plan");
    }
    if artifact.repository != repository_root()?.display().to_string()
        || artifact.head != commit_id("HEAD")?
        || artifact.index_sha256 != index_sha256()?
    {
        bail!("分析计划已过期；HEAD、index 或仓库路径发生变化");
    }
    let loaded = Plan::load_snapshots(revisions.each_ref().map(String::as_str))?;
    let snapshots = loaded.each_ref().map(|(_, snapshot)| snapshot);
    let computed = analysis::analyze(
        &[
            snapshots[0].clone(),
            snapshots[1].clone(),
            snapshots[2].clone(),
        ],
        loaded.each_ref().map(|(tree, _)| tree.clone()),
    )?;
    if serde_json::to_vec(&computed)? != serde_json::to_vec(&artifact.report)? {
        bail!("分析计划内容与当前 Git 对象不匹配；请重新执行 --plan");
    }
    Plan::from_loaded(artifact, target, loaded, options)
}
fn new_internal_path() -> Result<PathBuf> {
    let root = git_path("strict-weave")?;
    std::fs::create_dir_all(&root)?;
    Ok(tempfile::Builder::new()
        .prefix("operation-")
        .tempdir_in(root)?
        .keep()
        .join("plan.json"))
}
fn native(args: &[&str]) -> Result<Output> {
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
    let output = git(&prefix)?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    Ok(output)
}
fn finish_commit(args: &[&str]) -> Result<u8> {
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
fn execute_plan(plan: Plan, kind: &str, options: Options) -> Result<u8> {
    plan.recheck()?;
    let revisions = plan.artifact.revisions.each_ref().map(String::as_str);
    let result = match kind {
        "merge" => native(&[
            "merge",
            "--no-commit",
            "--no-ff",
            "--no-edit",
            "--no-autostash",
            "--no-overwrite-ignore",
            "-s",
            "ort",
            revisions[2],
        ])?,
        "cherry-pick" => native(&["cherry-pick", "--no-commit", "--strategy=ort", revisions[2]])?,
        _ => bail!("未知操作：{kind}"),
    };
    if !matches!(result.status.code(), Some(0 | 1)) {
        bail!("Git 未建立预期操作状态");
    }
    if kind == "merge" && !git_path("MERGE_HEAD")?.exists() {
        bail!("Git 未建立 MERGE_HEAD");
    }
    if kind == "cherry-pick" {
        repository::atomic_write(
            &git_path("CHERRY_PICK_HEAD")?,
            format!("{}\n", revisions[2]).as_bytes(),
        )?;
        let message = read(&["show", "-s", "--format=%B", revisions[2]])?;
        repository::atomic_write(&git_path("MERGE_MSG")?, &message)?;
    }
    if plan.install(options)? {
        return Ok(1);
    }
    match kind {
        "merge" => finish_commit(&["commit", "--no-edit"]),
        "cherry-pick" => finish_commit(&["commit", "--allow-empty", "-C", revisions[2]]),
        _ => unreachable!(),
    }
}
fn run(operation: &str, target: &str, options: Options, mode: Mode) -> Result<u8> {
    let _guard = enter()?;
    let ours = commit_id("HEAD")?;
    let theirs = commit_id(target)?;
    let base = if operation == "merge" {
        unique_base(&ours, &theirs)?
    } else {
        let parents = string(&["rev-list", "--parents", "-n", "1", &theirs])?;
        let words: Vec<_> = parents.split_whitespace().collect();
        if words.len() != 2 {
            bail!("cherry-pick 仅支持恰好一个 parent 的 commit");
        }
        words[1].into()
    };
    let revisions = [base, ours, theirs];
    let plan = match &mode {
        Mode::ApplyPlan(path) => {
            load_artifact(path, operation, target, revisions.clone(), options)?
        }
        Mode::Apply | Mode::Plan(_) => Plan::build(operation, target, revisions.clone(), options)?,
    };
    match mode {
        Mode::Plan(path) => {
            plan.save_new(&path)?;
            plan.report(options.detailed);
            eprintln!("分析计划：{}", path.display());
            Ok(u8::from(!plan.outcomes.is_empty()))
        }
        Mode::ApplyPlan(_) | Mode::Apply => {
            let path = if matches!(mode, Mode::Apply) {
                Some(new_internal_path()?)
            } else {
                None
            };
            if let Some(path) = path {
                plan.save_new(&path)?;
                eprintln!("分析计划：{}", path.display());
            }
            if operation == "merge" && revisions[0] == revisions[2] {
                return Ok(0);
            }
            if operation == "merge" && revisions[0] == revisions[1] {
                checked(native(&[
                    "merge",
                    "--ff-only",
                    "--no-autostash",
                    "--no-overwrite-ignore",
                    &revisions[2],
                ])?)?;
                return Ok(0);
            }
            execute_plan(plan, operation, options)
        }
    }
}
pub fn merge(target: &str, options: Options, mode: Mode) -> Result<u8> {
    run("merge", target, options, mode)
}
pub fn cherry_pick(target: &str, options: Options, mode: Mode) -> Result<u8> {
    run("cherry-pick", target, options, mode)
}
