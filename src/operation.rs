//! Restricted Git porcelain. Analysis and rendering precede repository writes;
//! Git owns native merge state, and strict conflicts get real index stages.
use crate::{analysis, merge, repository};
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub zdiff3: bool,
    pub detailed: bool,
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

// Git paths must be resolved through rev-parse (linked worktrees have a .git
// file). This lock excludes other strict-weave commands, not unrelated editors
// or Git processes: concurrent repository writes are unsupported.
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
    let root = string(&["rev-parse", "--show-toplevel"])?;
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
            bail!("已有 Git 操作：{name}；请先继续或中止该操作");
        }
    }
    let dir = git_path("strict-weave")?;
    std::fs::create_dir_all(&dir)?;
    let lock = dir.join("operation.lock");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .context("已有 strict-weave 操作；异常退出后须确认无运行进程再移除 operation.lock")?;
    let guard = Guard(lock);
    clean()?;
    Ok(guard)
}

fn check_subset(revisions: [&str; 3], paths: &BTreeSet<String>) -> Result<()> {
    // Initial scope is path-preserving text merges. Refuse whole-file renames
    // rather than install original-path stages over Git's renamed-path stages.
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
        let mut fields = status.split(|b| *b == 0).filter(|s| !s.is_empty());
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
    // Custom drivers, filters and checkout transformations would invalidate the
    // original-byte contract. Check each committed attributes snapshot AND the
    // effective worktree attributes (including .git/info/attributes).
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
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
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
    revisions: [String; 3],
    labels: merge::Labels,
    outcomes: BTreeMap<String, merge::Outcome>,
    stages: Vec<u8>,
    report_dir: PathBuf,
}
impl Plan {
    fn new(revisions: [String; 3], label: &str, options: Options) -> Result<Self> {
        let refs = revisions.each_ref().map(String::as_str);
        let loaded = [
            analysis::snapshot(refs[0])?,
            analysis::snapshot(refs[1])?,
            analysis::snapshot(refs[2])?,
        ];
        let trees = loaded.each_ref().map(|(id, _)| id.clone());
        let snapshots = loaded.map(|(_, snapshot)| snapshot);
        let paths = snapshots.iter().flat_map(|s| s.keys().cloned()).collect();
        check_subset(refs, &paths)?;
        let report = analysis::analyze(&snapshots, trees)?;
        let labels = merge::Labels {
            base: revisions[0].clone(),
            ours: revisions[1].clone(),
            theirs: label.into(),
        };
        let style = if options.zdiff3 {
            merge::ConflictStyle::Zdiff3
        } else {
            merge::ConflictStyle::Diff3
        };
        let mut outcomes = BTreeMap::new();
        let mut stages = Vec::new();
        for (path, global) in &report.files {
            let texts = snapshots
                .each_ref()
                .map(|s| s.get(path).map(String::as_str).unwrap_or(""));
            let mut outcome =
                merge::merge_with_style(texts[0], texts[1], texts[2], path, &labels, 7, style)?;
            analysis::augment(&mut outcome, global, texts, &labels, 7);
            if outcome.conflicted() {
                // NUL framing permits tabs/newlines in Git paths. Remove stage
                // zero first; None preserves absent-file vs empty-file identity.
                stages.extend_from_slice(
                    format!("0 {}\t{path}\0", "0".repeat(revisions[1].len())).as_bytes(),
                );
                for (i, revision) in refs.iter().enumerate() {
                    if let Some((mode, oid)) = analysis::blob_oid(revision, path)? {
                        stages.extend_from_slice(
                            format!("{mode} {oid} {}\t{path}\0", i + 1).as_bytes(),
                        );
                    }
                }
                outcomes.insert(path.clone(), outcome);
            }
        }
        let report_dir = tempfile::Builder::new()
            .prefix("operation-")
            .tempdir_in(git_path("strict-weave")?)?
            .keep();
        analysis::save(&report_dir.join("analysis.json"), &report)?;
        eprintln!("分析结果：{}", report_dir.join("analysis.json").display());
        Ok(Self {
            revisions,
            labels,
            outcomes,
            stages,
            report_dir,
        })
    }
    fn install(&self, options: Options) -> Result<bool> {
        // Mark index unresolved before writing markers: an interrupted write
        // cannot leave a path marked resolved while strict conflict is known.
        input(&["update-index", "-z", "--index-info"], &self.stages)?;
        for (path, outcome) in &self.outcomes {
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            repository::atomic_write(Path::new(path), outcome.content.as_bytes())?;
            repository::report(path, outcome, &self.labels, options.detailed);
        }
        let conflicts = !read(&["ls-files", "-u", "-z"])?.is_empty();
        std::fs::write(
            self.report_dir.join("applied"),
            if conflicts { "conflict\n" } else { "clean\n" },
        )?;
        Ok(conflicts)
    }
    fn recheck(&self) -> Result<()> {
        clean()?;
        if commit_id("HEAD")? != self.revisions[1] {
            bail!("分析期间 HEAD 已变化，拒绝执行");
        }
        // Check ignored collisions too: native merge's default can overwrite
        // ignored files, which must not happen to a strict-weave output.
        for path in self.outcomes.keys() {
            if analysis::blob_oid(&self.revisions[1], path)?.is_none() && Path::new(path).exists() {
                bail!("冲突输出会覆盖未跟踪文件：{path}");
            }
        }
        Ok(())
    }
}
fn native(args: &[&str]) -> Result<Output> {
    // No external hooks/drivers/rerere may resolve or commit between the native
    // merge and strict stage installation. Commit hooks run at final commit.
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

pub fn merge(target: &str, options: Options) -> Result<u8> {
    let _guard = enter()?;
    let ours = commit_id("HEAD")?;
    let theirs = commit_id(target)?;
    let base = unique_base(&ours, &theirs)?;
    if base == theirs {
        eprintln!("已经是最新版本");
        return Ok(0);
    }
    let plan = Plan::new([base.clone(), ours, theirs.clone()], target, options)?;
    plan.recheck()?;
    if base == plan.revisions[1] {
        checked(native(&[
            "merge",
            "--ff-only",
            "--no-autostash",
            "--no-overwrite-ignore",
            &theirs,
        ])?)?;
        return Ok(0);
    }
    let result = native(&[
        "merge",
        "--no-commit",
        "--no-ff",
        "--no-edit",
        "--no-autostash",
        "--no-overwrite-ignore",
        "-s",
        "ort",
        &theirs,
    ])?;
    if !matches!(result.status.code(), Some(0 | 1)) || !git_path("MERGE_HEAD")?.exists() {
        bail!("Git 未建立预期 merge 状态，未安装严格冲突；请检查 git status");
    }
    if plan.install(options)? {
        return Ok(1);
    }
    finish_commit(&["commit", "--no-edit"])
}

pub fn cherry_pick(target: &str, options: Options) -> Result<u8> {
    let _guard = enter()?;
    let ours = commit_id("HEAD")?;
    let theirs = commit_id(target)?;
    let parents = string(&["rev-list", "--parents", "-n", "1", &theirs])?;
    let words: Vec<_> = parents.split_whitespace().collect();
    if words.len() != 2 {
        bail!("cherry-pick 仅支持恰好一个 parent 的 commit");
    }
    let plan = Plan::new([words[1].into(), ours, theirs.clone()], target, options)?;
    plan.recheck()?;
    let result = native(&["cherry-pick", "--no-commit", "--strategy=ort", &theirs])?;
    if !matches!(result.status.code(), Some(0 | 1))
        || (!result.status.success() && read(&["ls-files", "-u", "-z"])?.is_empty())
    {
        bail!("Git cherry-pick 未完成预期合并；请检查 git status");
    }
    // -n does not always create CHERRY_PICK_HEAD. Install the documented
    // single-pick marker so git cherry-pick --continue/--abort work as usual.
    repository::atomic_write(
        &git_path("CHERRY_PICK_HEAD")?,
        format!("{theirs}\n").as_bytes(),
    )?;
    let message = read(&["show", "-s", "--format=%B", &theirs])?;
    repository::atomic_write(&git_path("MERGE_MSG")?, &message)?;
    if plan.install(options)? {
        return Ok(1);
    }
    finish_commit(&["commit", "--allow-empty", "-C", &theirs])
}
