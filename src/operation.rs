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
const REBASE_STATE_VERSION: u32 = 1;

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

#[derive(Clone, Copy, Eq, PartialEq)]
enum OperationKind {
    Merge,
    CherryPick,
    Rebase,
    Stash,
}

impl OperationKind {
    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::CherryPick => "cherry-pick",
            Self::Rebase => "rebase",
            Self::Stash => "stash",
        }
    }
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebasePlanArtifact {
    version: u32,
    operation: String,
    target: String,
    repository: String,
    original_head: String,
    upstream: String,
    index_sha256: String,
    commits: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebaseState {
    version: u32,
    repository: String,
    branch: String,
    original_head: String,
    upstream: String,
    commits: Vec<String>,
    next: usize,
    stopped: bool,
    stopped_head: Option<String>,
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
fn rebase_state_path() -> Result<PathBuf> {
    git_path("strict-weave/rebase-state.json")
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
    if rebase_state_path()?.exists() {
        bail!("已有 strict-weave rebase；请使用 rebase --continue 或 --abort");
    }
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

fn enter_rebase_control() -> Result<Guard> {
    if std::env::var_os("GIT_INDEX_FILE").is_some() {
        bail!("不支持 GIT_INDEX_FILE");
    }
    let root = repository_root()?;
    std::env::set_current_dir(root)?;
    let state = rebase_state_path()?;
    if !state.exists() {
        bail!("没有进行中的 strict-weave rebase");
    }
    let dir = git_path("strict-weave")?;
    std::fs::create_dir_all(&dir)?;
    let lock = dir.join("operation.lock");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .context("已有 strict-weave 操作；确认无运行进程后再移除 operation.lock")?;
    Ok(Guard(lock))
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
        operation: OperationKind,
        target: &str,
        label: &str,
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
            operation: operation.name().into(),
            target: target.into(),
            repository: repository_root()?.display().to_string(),
            revisions,
            head: commit_id("HEAD")?,
            index_sha256: index_sha256()?,
            report,
        };
        Self::from_loaded(artifact, label, loaded, options)
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
        if commit_id("HEAD")? != self.artifact.head {
            bail!("分析期间 HEAD 已变化，拒绝执行");
        }
        if index_sha256()? != self.artifact.index_sha256 {
            bail!("分析期间 Git index 已变化，拒绝执行");
        }
        if repository_root()?.display().to_string() != self.artifact.repository {
            bail!("分析计划属于另一个仓库");
        }
        let expected_target = if self.artifact.operation == "rebase" {
            &self.artifact.revisions[1]
        } else {
            &self.artifact.revisions[2]
        };
        if commit_id(&self.artifact.target)? != *expected_target {
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

fn make_plan(
    mode: &Mode,
    operation: OperationKind,
    target: &str,
    label: &str,
    revisions: [String; 3],
    options: Options,
) -> Result<Plan> {
    match mode {
        Mode::ApplyPlan(path) => load_artifact(path, operation, target, revisions, options),
        Mode::Apply | Mode::Plan(_) => Plan::build(operation, target, label, revisions, options),
    }
}

fn report_plan(plan: &Plan, mode: &Mode, options: Options) -> Result<Option<u8>> {
    let Mode::Plan(path) = mode else {
        return Ok(None);
    };
    plan.save_new(path)?;
    plan.report(options.detailed);
    eprintln!("分析计划：{}", path.display());
    Ok(Some(u8::from(!plan.outcomes.is_empty())))
}

fn save_internal_plan(plan: &Plan, mode: &Mode) -> Result<()> {
    if !matches!(mode, Mode::Apply) {
        return Ok(());
    }
    let path = new_internal_path()?;
    plan.save_new(&path)?;
    eprintln!("分析计划：{}", path.display());
    Ok(())
}

fn load_artifact(
    path: &Path,
    operation: OperationKind,
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
        || artifact.operation != operation.name()
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
    let label = if operation == OperationKind::Rebase {
        artifact.revisions[2].clone()
    } else {
        target.to_owned()
    };
    Plan::from_loaded(artifact, &label, loaded, options)
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

fn save_rebase_state(state: &RebaseState) -> Result<()> {
    let path = rebase_state_path()?;
    let mut bytes = serde_json::to_vec_pretty(state)?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("rebase 状态超过 64 MiB");
    }
    bytes.push(b'\n');
    repository::atomic_write(&path, &bytes)
}

fn load_rebase_state() -> Result<RebaseState> {
    let path = rebase_state_path()?;
    let bytes = std::fs::read(&path).context("读取 strict-weave rebase 状态")?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("rebase 状态超过 64 MiB");
    }
    let state: RebaseState = serde_json::from_slice(&bytes).context("无效的 rebase 状态")?;
    if state.version != REBASE_STATE_VERSION
        || state.repository != repository_root()?.display().to_string()
        || state.commits.is_empty()
        || state.next > state.commits.len()
    {
        bail!("strict-weave rebase 状态不完整或不属于当前仓库");
    }
    Ok(state)
}

fn save_rebase_plan(path: &Path, plan: &RebasePlanArtifact) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(plan)?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("rebase 计划超过 64 MiB");
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

fn remove_rebase_state() -> Result<()> {
    let path = rebase_state_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
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

fn current_branch() -> Result<String> {
    string(&["symbolic-ref", "-q", "HEAD"]).context("strict-weave rebase 当前需要在 branch 上执行")
}

fn rebase_commits(upstream: &str, original: &str) -> Result<Vec<String>> {
    let output = string(&["rev-list", "--reverse", &format!("{upstream}..{original}")])?;
    let commits: Vec<_> = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    for commit in &commits {
        let parents = string(&["rev-list", "--parents", "-n", "1", commit])?;
        if parents.split_whitespace().count() != 2 {
            bail!("普通 rebase 不支持 merge commit：{commit}");
        }
    }
    Ok(commits)
}

fn clear_cherry_pick_state() -> Result<()> {
    let cherry_head = git_path("CHERRY_PICK_HEAD")?;
    if cherry_head.exists() {
        std::fs::remove_file(cherry_head)?;
    }
    let merge_msg = git_path("MERGE_MSG")?;
    if merge_msg.exists() {
        std::fs::remove_file(merge_msg)?;
    }
    Ok(())
}

fn ensure_resolved_rebase_index() -> Result<()> {
    if !read(&["ls-files", "-u", "-z"])?.is_empty() {
        bail!("仍有未解决的 index 冲突；请先编辑并 git add 文件");
    }
    let status = read(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    for entry in status
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        if entry.len() < 2 {
            bail!("Git status 输出不完整");
        }
        if entry[0] == b'?' || entry[1] != b' ' {
            bail!("存在未暂存或未跟踪的修改；请只保留已 git add 的 rebase 结果");
        }
    }
    Ok(())
}

fn save_rebase_operation_plan(
    mode: &Mode,
    target: &str,
    original: &str,
    upstream: &str,
    commits: &[String],
) -> Result<Option<u8>> {
    let artifact = RebasePlanArtifact {
        version: REBASE_STATE_VERSION,
        operation: OperationKind::Rebase.name().into(),
        target: target.into(),
        repository: repository_root()?.display().to_string(),
        original_head: original.into(),
        upstream: upstream.into(),
        index_sha256: index_sha256()?,
        commits: commits.to_vec(),
    };
    let path = match mode {
        Mode::Plan(path) => path,
        Mode::Apply => return Ok(None),
        Mode::ApplyPlan(_) => return Ok(None),
    };
    save_rebase_plan(path, &artifact)?;
    eprintln!(
        "重放计划：{}（{} 个 commit）",
        path.display(),
        commits.len()
    );
    Ok(Some(0))
}

fn load_rebase_operation_plan(
    path: &Path,
    target: &str,
    original: &str,
    upstream: &str,
    commits: &[String],
) -> Result<()> {
    let bytes = std::fs::read(path).context("读取 rebase 计划")?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("rebase 计划超过 64 MiB");
    }
    let artifact: RebasePlanArtifact =
        serde_json::from_slice(&bytes).context("无效的 rebase 计划")?;
    if artifact.version != REBASE_STATE_VERSION
        || artifact.operation != OperationKind::Rebase.name()
        || artifact.target != target
        || artifact.repository != repository_root()?.display().to_string()
        || artifact.original_head != original
        || artifact.upstream != upstream
        || artifact.index_sha256 != index_sha256()?
        || artifact.commits != commits
    {
        bail!("rebase 计划已过期或与当前操作不匹配；请重新执行 --plan");
    }
    Ok(())
}

fn finish_rebase(state: &RebaseState) -> Result<u8> {
    let head = commit_id("HEAD")?;
    read(&["update-ref", &state.branch, &head])?;
    read(&["symbolic-ref", "HEAD", &state.branch])?;
    remove_rebase_state()?;
    eprintln!("rebase 完成：重放 {} 个 commit", state.commits.len());
    Ok(0)
}

fn advance_rebase(state: &mut RebaseState, options: Options) -> Result<u8> {
    loop {
        if state.next == state.commits.len() {
            return finish_rebase(state);
        }
        if state.stopped {
            return Ok(1);
        }
        let commit = state.commits[state.next].clone();
        let parent = commit_id(&format!("{commit}^"))?;
        let ours = commit_id("HEAD")?;
        let revisions = [parent, ours.clone(), commit.clone()];
        let plan = Plan::build(OperationKind::Rebase, &ours, &commit, revisions, options)?;
        plan.recheck()?;
        save_internal_plan(&plan, &Mode::Apply)?;

        state.stopped = true;
        state.stopped_head = Some(ours.clone());
        save_rebase_state(state)?;
        let result = native(&["cherry-pick", "--no-commit", "--strategy=ort", &commit])?;
        if !matches!(result.status.code(), Some(0 | 1)) {
            bail!("Git 无法应用 rebase commit；状态已保留，可使用 --abort");
        }
        clear_cherry_pick_state()?;
        if plan.install(options)? {
            eprintln!(
                "rebase 在第 {} 个 commit 处暂停；请解决后运行 strict-weave rebase --continue",
                state.next + 1
            );
            return Ok(1);
        }
        finish_commit(&["commit", "--allow-empty", "-C", &commit])?;
        state.next += 1;
        state.stopped = false;
        state.stopped_head = None;
        save_rebase_state(state)?;
    }
}
fn execute_plan(plan: Plan, kind: OperationKind, options: Options) -> Result<u8> {
    plan.recheck()?;
    let revisions = plan.artifact.revisions.each_ref().map(String::as_str);
    let result = match kind {
        OperationKind::Merge => native(&[
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
        OperationKind::CherryPick => {
            native(&["cherry-pick", "--no-commit", "--strategy=ort", revisions[2]])?
        }
        _ => bail!("该操作不能使用通用 apply 流程：{}", kind.name()),
    };
    if !matches!(result.status.code(), Some(0 | 1)) {
        bail!("Git 未建立预期操作状态");
    }
    if kind == OperationKind::Merge && !git_path("MERGE_HEAD")?.exists() {
        bail!("Git 未建立 MERGE_HEAD");
    }
    if kind == OperationKind::CherryPick {
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
        OperationKind::Merge => finish_commit(&["commit", "--no-edit"]),
        OperationKind::CherryPick => {
            finish_commit(&["commit", "--allow-empty", "-C", revisions[2]])
        }
        _ => unreachable!(),
    }
}
fn run(operation: OperationKind, target: &str, options: Options, mode: Mode) -> Result<u8> {
    let _guard = enter()?;
    let ours = commit_id("HEAD")?;
    let theirs = commit_id(target)?;
    let base = if operation == OperationKind::Merge {
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
    let plan = make_plan(&mode, operation, target, target, revisions.clone(), options)?;
    if let Some(code) = report_plan(&plan, &mode, options)? {
        return Ok(code);
    }
    save_internal_plan(&plan, &mode)?;
    if operation == OperationKind::Merge && revisions[0] == revisions[2] {
        return Ok(0);
    }
    if operation == OperationKind::Merge && revisions[0] == revisions[1] {
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
pub fn merge(target: &str, options: Options, mode: Mode) -> Result<u8> {
    run(OperationKind::Merge, target, options, mode)
}
pub fn cherry_pick(target: &str, options: Options, mode: Mode) -> Result<u8> {
    run(OperationKind::CherryPick, target, options, mode)
}

pub fn rebase(target: &str, options: Options, mode: Mode) -> Result<u8> {
    let _guard = enter()?;
    let original = commit_id("HEAD")?;
    let branch = current_branch()?;
    let upstream = commit_id(target)?;
    let commits = rebase_commits(&upstream, &original)?;
    if commits.is_empty() {
        eprintln!("rebase：没有需要重放的 commit");
        return Ok(0);
    }
    if let Some(code) = save_rebase_operation_plan(&mode, target, &original, &upstream, &commits)? {
        return Ok(code);
    }
    if let Mode::ApplyPlan(path) = &mode {
        load_rebase_operation_plan(path, target, &original, &upstream, &commits)?;
    }
    if let Mode::Apply = mode {
        let path = new_internal_path()?;
        let artifact = RebasePlanArtifact {
            version: REBASE_STATE_VERSION,
            operation: OperationKind::Rebase.name().into(),
            target: target.into(),
            repository: repository_root()?.display().to_string(),
            original_head: original.clone(),
            upstream: upstream.clone(),
            index_sha256: index_sha256()?,
            commits: commits.clone(),
        };
        save_rebase_plan(&path, &artifact)?;
        eprintln!(
            "重放计划：{}（{} 个 commit）",
            path.display(),
            commits.len()
        );
    }
    let checkout = native(&["checkout", "--detach", &upstream])?;
    if !checkout.status.success() {
        bail!("无法将 rebase 基底切换到 upstream");
    }
    read(&["update-ref", "ORIG_HEAD", &original])?;
    let mut state = RebaseState {
        version: REBASE_STATE_VERSION,
        repository: repository_root()?.display().to_string(),
        branch,
        original_head: original,
        upstream,
        commits,
        next: 0,
        stopped: false,
        stopped_head: None,
    };
    save_rebase_state(&state)?;
    advance_rebase(&mut state, options)
}

pub fn rebase_continue(options: Options) -> Result<u8> {
    let _guard = enter_rebase_control()?;
    let mut state = load_rebase_state()?;
    if !state.stopped {
        bail!("当前 rebase 没有等待解决的 commit");
    }
    let expected_head = state
        .stopped_head
        .as_deref()
        .context("rebase 状态缺少暂停时的 HEAD")?;
    if commit_id("HEAD")? != expected_head {
        bail!("rebase 暂停后的 HEAD 已变化；请使用 --abort 或恢复该 HEAD");
    }
    ensure_resolved_rebase_index()?;
    let commit = state
        .commits
        .get(state.next)
        .context("rebase 状态的 commit 序号无效")?
        .clone();
    clear_cherry_pick_state()?;
    finish_commit(&["commit", "--allow-empty", "-C", &commit])?;
    state.next += 1;
    state.stopped = false;
    state.stopped_head = None;
    save_rebase_state(&state)?;
    advance_rebase(&mut state, options)
}

pub fn rebase_abort() -> Result<u8> {
    let _guard = enter_rebase_control()?;
    let state = load_rebase_state()?;
    native(&["reset", "--hard", &state.original_head])?;
    read(&["update-ref", &state.branch, &state.original_head])?;
    read(&["symbolic-ref", "HEAD", &state.branch])?;
    clear_cherry_pick_state()?;
    remove_rebase_state()?;
    eprintln!("rebase 已中止，恢复到 {}", state.original_head);
    Ok(0)
}

pub fn stash(target: &str, pop: bool, options: Options, mode: Mode) -> Result<u8> {
    let _guard = enter()?;
    let ours = commit_id("HEAD")?;
    let stash = commit_id(target)?;
    let parents = string(&["rev-list", "--parents", "-n", "1", &stash])?;
    let words: Vec<_> = parents.split_whitespace().collect();
    if words.len() != 3 {
        bail!("当前阶段不支持包含未跟踪文件的 stash");
    }
    let base = words[1].to_owned();
    let index_parent = words[2];
    let revisions = [base, ours, stash.clone()];
    let plan = make_plan(
        &mode,
        OperationKind::Stash,
        target,
        target,
        revisions.clone(),
        options,
    )?;
    // Applying a stash without --index has no reliable representation for a
    // separately changed index in this restricted command. Reject it before
    // touching the worktree; users can use native Git for that case.
    if string(&["rev-parse", &format!("{index_parent}^{{tree}}")])?
        != string(&["rev-parse", &format!("{}^{{tree}}", revisions[0])])?
    {
        bail!("当前阶段不支持 stash 的独立 index 修改；请使用原生 Git");
    }
    if let Some(code) = report_plan(&plan, &mode, options)? {
        return Ok(code);
    }
    save_internal_plan(&plan, &mode)?;
    plan.recheck()?;
    let result = native(&["stash", "apply", "--quiet", target])?;
    if !matches!(result.status.code(), Some(0 | 1)) {
        bail!("Git stash apply 未完成；未安装严格结果");
    }
    if plan.install(options)? {
        return Ok(1);
    }
    if pop {
        let dropped = native(&["stash", "drop", "--quiet", target])?;
        if !dropped.status.success() {
            bail!("stash 已应用，但 drop 失败；请检查 stash list");
        }
    }
    Ok(0)
}
