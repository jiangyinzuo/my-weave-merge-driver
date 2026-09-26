//! Linear replay with fresh global analysis at every step, including after a
//! manual resolution. Git's sequencer is not used: it would bypass that analysis.
use super::{
    git::{
        acquire, checked, clean, commit_id, enter, git_path, head_branch, native, native_apply,
        read, replay_commit, repository_root, string, unique_base,
    },
    plan::{make_plan, report_plan, save_internal_plan, save_new, Plan},
    Mode, OperationKind, Options,
};
use anyhow::{bail, Context, Result};
use plan::Preview;
use state::{ensure_resolved, Phase, State};

mod plan;
mod state;

fn commits_to_replay(upstream: &str, original: &str) -> Result<Vec<String>> {
    let commits: Vec<_> = string(&["rev-list", "--reverse", &format!("{upstream}..{original}")])?
        .lines()
        .map(str::to_owned)
        .collect();
    for commit in &commits {
        if string(&["rev-list", "--parents", "-n", "1", commit])?
            .split_whitespace()
            .count()
            != 2
        {
            bail!("普通 rebase 仅支持单 parent commit：{commit}");
        }
    }
    Ok(commits)
}

fn replay_plan(ours: &str, commit: &str, options: Options) -> Result<Plan> {
    Plan::build(
        OperationKind::Rebase,
        commit,
        commit,
        [
            commit_id(&format!("{commit}^"))?,
            ours.into(),
            commit.into(),
        ],
        options,
    )
}

fn clear_pick_state() -> Result<()> {
    for name in ["CHERRY_PICK_HEAD", "MERGE_MSG"] {
        let path = git_path(name)?;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn start(state: &mut State) -> Result<()> {
    state.validate_context()?;
    // Check the first three snapshots before checkout. Ready repeats analysis
    // against the actual detached HEAD and saves a plan bound to that index.
    let first = state.commits.first().context("rebase 缺少待重放 commit")?;
    replay_plan(&state.upstream, first, state.options)?.recheck()?;
    read(&["update-ref", "ORIG_HEAD", &state.original_head])?;
    checked(native(&[
        "checkout",
        "--detach",
        "--no-overwrite-ignore",
        &state.upstream,
    ])?)?;
    state.phase = Phase::Ready;
    state.save()
}

fn apply_next(state: &mut State) -> Result<bool> {
    let commit = state.commits[state.next].clone();
    let plan = replay_plan(&state.head, &commit, state.options)?;
    plan.recheck()?;
    save_internal_plan(&plan, &Mode::Apply)?;
    state.validate_context()?;
    // If Git or file installation fails, Applied is never written. Continue
    // cannot mistake partial installation for a user's completed resolution.
    state.phase = Phase::Applying;
    state.save()?;
    native_apply(&["cherry-pick", "--no-commit", "--strategy=ort", &commit])
        .context("rebase 应用未完成；状态已保留，请使用 strict-weave rebase --abort")?;
    state.validate_context()?;
    clear_pick_state()?;
    let conflicted = plan.install(state.options)?;
    state.phase = Phase::Applied;
    state.save()?;
    Ok(conflicted)
}

fn prepare_commit(state: &mut State) -> Result<()> {
    ensure_resolved()?;
    let tree = string(&["write-tree"])?;
    let commit = replay_commit(&tree, &state.head, &state.commits[state.next])?;
    state.phase = Phase::Committing { commit };
    state.save()
}

fn complete_commit(state: &mut State, commit: &str) -> Result<()> {
    ensure_resolved()?;
    if string(&["write-tree"])? != string(&["rev-parse", &format!("{commit}^{{tree}}")])?
        || commit_id(&format!("{commit}^"))? != state.head
    {
        bail!("提交期间 index 或保存的 commit 已变化；请恢复暂存结果后重试");
    }
    if commit_id("HEAD")? != commit {
        read(&[
            "update-ref",
            "--no-deref",
            "-m",
            "strict-weave rebase: replay",
            "HEAD",
            commit,
            &state.head,
        ])?;
    }
    state.head = commit.into();
    state.next += 1;
    state.phase = Phase::Ready;
    state.save()
}

fn finish(state: State) -> Result<u8> {
    clean()?;
    if commit_id(&state.branch)? != state.head {
        read(&[
            "update-ref",
            "-m",
            "strict-weave rebase: finish",
            &state.branch,
            &state.head,
            &state.original_head,
        ])?;
    }
    read(&["symbolic-ref", "HEAD", &state.branch])?;
    eprintln!("rebase 完成：重放 {} 个 commit", state.commits.len());
    state.remove()?;
    Ok(0)
}

fn advance(mut state: State) -> Result<u8> {
    loop {
        state.validate_context()?;
        match &state.phase {
            Phase::Starting => start(&mut state)?,
            Phase::Ready => {
                clean()?;
                if state.next == state.commits.len() {
                    state.phase = Phase::Finishing;
                    state.save()?;
                } else if apply_next(&mut state)? {
                    eprintln!("rebase 在第 {}/{} 个 commit 处暂停；解决并 git add 后运行 strict-weave rebase --continue；中止用 --abort",
                        state.next + 1, state.commits.len());
                    return Ok(1);
                }
            }
            Phase::Applying => bail!(
                "上次 rebase 应用未完整结束，不能继续提交；请使用 strict-weave rebase --abort"
            ),
            Phase::Applied => prepare_commit(&mut state)?,
            Phase::Committing { commit } => {
                let commit = commit.clone();
                complete_commit(&mut state, &commit)?;
            }
            Phase::Finishing => return finish(state),
            Phase::Aborting { .. } => bail!("rebase 正在中止；请重试 strict-weave rebase --abort"),
        }
    }
}

pub fn rebase(target: &str, options: Options, mode: Mode) -> Result<u8> {
    let (_guard, mode) = enter(mode)?;
    let original = commit_id("HEAD")?;
    let branch = head_branch()?.context("strict-weave rebase 当前需要在 branch 上执行")?;
    let upstream = commit_id(target)?;
    let base = unique_base(&original, &upstream)?;
    // Already based on upstream, or only a fast-forward is needed.
    if base == upstream || base == original {
        let plan = make_plan(
            &mode,
            OperationKind::Rebase,
            target,
            target,
            [base.clone(), original, upstream.clone()],
            options,
        )?;
        if let Some(code) = report_plan(&plan, &mode, options)? {
            return Ok(code);
        }
        save_internal_plan(&plan, &mode)?;
        plan.recheck()?;
        if base != upstream {
            checked(native(&[
                "merge",
                "--ff-only",
                "--no-autostash",
                "--no-overwrite-ignore",
                &upstream,
            ])?)?;
        }
        return Ok(0);
    }
    let commits = commits_to_replay(&upstream, &original)?;
    let state = State {
        version: state::VERSION,
        repository: repository_root()?.display().to_string(),
        branch,
        original_head: original,
        head: upstream.clone(),
        upstream,
        commits,
        next: 0,
        options,
        phase: Phase::Starting,
    };
    match &mode {
        Mode::Plan(path) => return Preview::build(target, &state)?.save(path, options.detailed),
        Mode::ApplyPlan(path) => Preview::build(target, &state)?.verify_file(path)?,
        Mode::Apply => (),
    }
    // Persist recovery information before the first checkout or ref change.
    save_new(&state::path()?, &state)?;
    advance(state)
}

pub fn rebase_continue(options: Options) -> Result<u8> {
    let _guard = acquire()?;
    let mut state = State::load()?;
    // Continuation inherits display options. Supplied flags may enable them.
    state.options.zdiff3 |= options.zdiff3;
    state.options.detailed |= options.detailed;
    state.save()?;
    advance(state)
}

pub fn rebase_abort() -> Result<u8> {
    let _guard = acquire()?;
    let mut state = State::load()?;
    if !matches!(state.phase, Phase::Aborting { .. }) {
        state.phase = Phase::Aborting {
            head: commit_id("HEAD")?,
            branch_tip: commit_id(&state.branch)?,
        };
        state.save()?;
    }
    // A failed reset leaves Aborting intact. Never reattach/delete recovery
    // state unless Git actually restored both index and worktree.
    checked(native(&["reset", "--hard", &state.original_head])?)?;
    state.validate_context()?;
    let old_tip = commit_id(&state.branch)?;
    if old_tip != state.original_head {
        read(&["update-ref", &state.branch, &state.original_head, &old_tip])?;
    }
    read(&["symbolic-ref", "HEAD", &state.branch])?;
    clear_pick_state()?;
    eprintln!("rebase 已中止，恢复到 {}", state.original_head);
    state.remove()?;
    Ok(0)
}
