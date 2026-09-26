//! Single-target merge and cherry-pick.
use super::{
    git::{checked, commit_id, enter, finish_commit, git_path, native, read, string, unique_base},
    plan::{make_plan, report_plan, save_internal_plan, Plan},
    Mode, OperationKind, Options,
};
use crate::repository;
use anyhow::{bail, Result};

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
    plan.recheck()?;
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
