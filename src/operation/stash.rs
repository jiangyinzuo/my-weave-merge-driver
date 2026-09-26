//! Git stash apply/pop operation.
use super::{
    git::{commit_id, enter, native, string},
    plan::{make_plan, report_plan, save_internal_plan},
    Mode, OperationKind, Options,
};
use anyhow::{bail, Result};

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
