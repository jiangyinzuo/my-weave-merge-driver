//! Bound, immutable analysis plans and conflict installation.
use super::{
    git::{clean, commit_id, git_path, index_sha256, input, read, repository_root, string},
    Mode, OperationKind, Options,
};
use crate::{analysis, merge, repository};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const ARTIFACT_VERSION: u32 = 1;
pub(super) const MAX_ARTIFACT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    artifact_version: u32,
    operation: String,
    target: String,
    repository: String,
    pub(super) revisions: [String; 3],
    head: String,
    index_sha256: String,
    pub(super) report: analysis::Report,
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

pub(super) struct Plan {
    pub(super) artifact: Artifact,
    labels: merge::Labels,
    pub(super) outcomes: BTreeMap<String, merge::Outcome>,
    stages: Vec<u8>,
}
impl Plan {
    fn load_snapshots(revisions: [&str; 3]) -> Result<[analysis::GitSnapshot; 3]> {
        let loaded = [
            analysis::GitSnapshot::load(revisions[0])?,
            analysis::GitSnapshot::load(revisions[1])?,
            analysis::GitSnapshot::load(revisions[2])?,
        ];
        let paths = loaded
            .iter()
            .flat_map(|snapshot| snapshot.texts.keys().cloned())
            .collect();
        check_subset(revisions, &paths)?;
        Ok(loaded)
    }
    fn from_loaded(
        artifact: Artifact,
        label: &str,
        loaded: [analysis::GitSnapshot; 3],
        options: Options,
    ) -> Result<Self> {
        let revisions = artifact.revisions.each_ref().map(String::as_str);
        let trees = loaded.each_ref().map(|snapshot| snapshot.tree_id.clone());
        if artifact.report.trees != trees {
            bail!("分析报告的 tree ID 与当前 Git 对象不匹配");
        }
        let snapshots = loaded.each_ref().map(|snapshot| &snapshot.texts);
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
                for (stage, snapshot) in loaded.iter().enumerate() {
                    if let Some(entry) = snapshot.entries.get(path) {
                        stages.extend_from_slice(
                            format!("{} {} {}\t{path}\0", entry.mode, entry.oid, stage + 1)
                                .as_bytes(),
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
    pub(super) fn build(
        operation: OperationKind,
        target: &str,
        label: &str,
        revisions: [String; 3],
        options: Options,
    ) -> Result<Self> {
        let refs = revisions.each_ref().map(String::as_str);
        let loaded = Self::load_snapshots(refs)?;
        let trees = loaded.each_ref().map(|snapshot| snapshot.tree_id.clone());
        let snapshots = loaded.each_ref().map(|snapshot| &snapshot.texts);
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
    pub(super) fn save_new(&self, path: &Path) -> Result<()> {
        save_new(path, &self.artifact)
    }
    pub(super) fn report(&self, detailed: bool) {
        repository::report_analysis(&self.artifact.report, detailed);
    }
    pub(super) fn recheck(&self) -> Result<()> {
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
        if commit_id(&self.artifact.target)? != self.artifact.revisions[2] {
            bail!("分析计划的 target 已变化，拒绝执行");
        }
        Ok(())
    }
    pub(super) fn install(&self, options: Options) -> Result<bool> {
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

pub(super) fn make_plan(
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

pub(super) fn report_plan(plan: &Plan, mode: &Mode, options: Options) -> Result<Option<u8>> {
    let Mode::Plan(path) = mode else {
        return Ok(None);
    };
    plan.save_new(path)?;
    plan.report(options.detailed);
    eprintln!("分析计划：{}", path.display());
    Ok(Some(u8::from(!plan.outcomes.is_empty())))
}

pub(super) fn save_internal_plan(plan: &Plan, mode: &Mode) -> Result<()> {
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
    let artifact: Artifact = read_json(path).context("读取分析计划")?;
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
    let snapshots = loaded.each_ref().map(|snapshot| &snapshot.texts);
    let computed = analysis::analyze(
        &[
            snapshots[0].clone(),
            snapshots[1].clone(),
            snapshots[2].clone(),
        ],
        loaded.each_ref().map(|snapshot| snapshot.tree_id.clone()),
    )?;
    if serde_json::to_vec(&computed)? != serde_json::to_vec(&artifact.report)? {
        bail!("分析计划内容与当前 Git 对象不匹配；请重新执行 --plan");
    }
    Plan::from_loaded(artifact, target, loaded, options)
}
pub(super) fn new_internal_path() -> Result<PathBuf> {
    let root = git_path("strict-weave")?;
    std::fs::create_dir_all(&root)?;
    Ok(tempfile::Builder::new()
        .prefix("operation-")
        .tempdir_in(root)?
        .keep()
        .join("plan.json"))
}

/// Shared bounded JSON I/O for plans and rebase recovery state.
pub(super) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_ARTIFACT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        bail!("计划或状态超过 64 MiB");
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn save_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() as u64 + 1 > MAX_ARTIFACT {
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
