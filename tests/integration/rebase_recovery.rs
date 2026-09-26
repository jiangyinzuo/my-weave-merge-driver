use super::*;

fn conflicted_rebase() -> Repo {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.tool_code(&["rebase", "feature"], 1);
    repo
}

#[test]
fn continue_requires_resolved_index_and_staged_worktree() {
    let repo = conflicted_rebase();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let unresolved = repo.tool_code(&["rebase", "--continue"], 129);
    assert!(String::from_utf8_lossy(&unresolved.stderr).contains("未解决的 index 冲突"));
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.write("calc.go", BASE);
    let unstaged = repo.tool_code(&["rebase", "--continue"], 129);
    assert!(String::from_utf8_lossy(&unstaged.stderr).contains("未暂存"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    repo.git(&["add", "calc.go"]);
    repo.write("untracked.go", "package untracked\n");
    repo.tool_code(&["rebase", "--continue"], 129);
    fs::remove_file(repo.path().join("untracked.go")).unwrap();
    repo.tool_code(&["rebase", "--continue"], 0);
    let finished = repo.git(&["rev-parse", "HEAD"]);
    repo.tool_code(&["rebase", "--continue"], 129);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), finished);
}

#[test]
fn abort_failure_preserves_recovery_state_for_retry() {
    let repo = conflicted_rebase();
    let original = repo.git(&["rev-parse", "main"]);
    let detached = repo.git(&["rev-parse", "HEAD"]);
    repo.write(".git/index.lock", "test holds the index lock\n");
    repo.tool_code(&["rebase", "--abort"], 129);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), detached);
    assert_eq!(repo.git(&["rev-parse", "main"]), original);
    assert_eq!(repo.rebase_state()["phase"]["kind"], "aborting");
    repo.tool_code(&["rebase", "--continue"], 129);
    fs::remove_file(repo.path().join(".git/index.lock")).unwrap();
    repo.tool_code(&["rebase", "--abort"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), original);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}

#[test]
fn recorded_commit_recovers_before_and_after_the_head_update_without_replaying_twice() {
    let repo = conflicted_rebase();
    let old = repo.git(&["rev-parse", "HEAD"]);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.write(".git/HEAD.lock", "test holds the HEAD lock\n");
    repo.tool_code(&["rebase", "--continue"], 129);
    let state = repo.rebase_state();
    assert_eq!(state["phase"]["kind"], "committing");
    let new = state["phase"]["commit"].as_str().unwrap();
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), old);
    // Simulate the next crash boundary: HEAD updated, state not yet advanced.
    fs::remove_file(repo.path().join(".git/HEAD.lock")).unwrap();
    repo.git(&["update-ref", "--no-deref", "HEAD", new, &old]);
    repo.tool_code(&["rebase", "--continue"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), new);
    assert_eq!(repo.git(&["rev-parse", "main"]), new);
    assert_eq!(repo.git(&["rev-list", "--count", "feature..HEAD"]), "1");
}

#[test]
fn final_branch_update_failure_can_be_retried() {
    let repo = conflicted_rebase();
    let original = repo.git(&["rev-parse", "main"]);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.write(".git/refs/heads/main.lock", "test holds the branch lock\n");
    repo.tool_code(&["rebase", "--continue"], 129);
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert_eq!(repo.git(&["rev-parse", "main"]), original);
    assert_eq!(repo.rebase_state()["phase"]["kind"], "finishing");
    fs::remove_file(repo.path().join(".git/refs/heads/main.lock")).unwrap();
    repo.tool_code(&["rebase", "--continue"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
}

#[test]
fn abort_can_retry_after_finishing_reattached_head() {
    let repo = conflicted_rebase();
    let original = repo.git(&["rev-parse", "main"]);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.write(".git/refs/heads/main.lock", "test holds the branch lock\n");
    repo.tool_code(&["rebase", "--continue"], 129);
    assert_eq!(repo.rebase_state()["phase"]["kind"], "finishing");
    fs::remove_file(repo.path().join(".git/refs/heads/main.lock")).unwrap();
    let head = repo.git(&["rev-parse", "HEAD"]);
    // Simulate interruption after refs were restored, before state cleanup.
    repo.git(&["update-ref", "refs/heads/main", &head, &original]);
    repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
    repo.write(".git/index.lock", "test holds the index lock\n");
    repo.tool_code(&["rebase", "--abort"], 129);
    assert_eq!(repo.rebase_state()["phase"]["kind"], "aborting");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    fs::remove_file(repo.path().join(".git/index.lock")).unwrap();
    repo.tool_code(&["rebase", "--abort"], 0);
    assert_eq!(repo.git(&["rev-parse", "main"]), original);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}

#[test]
fn changed_head_branch_or_foreign_git_operation_is_never_overwritten() {
    for change in ["switched", "original-tip", "foreign-operation"] {
        let repo = conflicted_rebase();
        let original = repo.git(&["rev-parse", "main"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        repo.write("calc.go", OURS);
        repo.git(&["add", "calc.go"]);
        match change {
            "switched" => {
                repo.git(&["checkout", "-qb", "other"]);
            }
            "original-tip" => {
                repo.git(&["update-ref", "refs/heads/main", &head, &original]);
            }
            _ => repo.write(".git/MERGE_HEAD", &format!("{original}\n")),
        }
        let refs = repo.git(&["show-ref"]);
        let index = fs::read(repo.path().join(".git/index")).unwrap();
        repo.tool_code(&["rebase", "--continue"], 129);
        repo.tool_code(&["rebase", "--abort"], 129);
        assert_eq!(repo.git(&["show-ref"]), refs);
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
        assert_eq!(repo.content("calc.go"), OURS);
        match change {
            "switched" => {
                repo.git(&["checkout", "--detach"]);
            }
            "original-tip" => {
                repo.git(&["update-ref", "refs/heads/main", &original, &head]);
            }
            _ => fs::remove_file(repo.path().join(".git/MERGE_HEAD")).unwrap(),
        }
        repo.tool_code(&["rebase", "--abort"], 0);
    }
}

#[test]
fn interrupted_apply_is_not_mistaken_for_a_resolved_conflict() {
    let repo = conflicted_rebase();
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    // Fault injection: install did not reach its durable Applied checkpoint.
    let mut state = repo.rebase_state();
    state["phase"]["kind"] = serde_json::json!("applying");
    repo.write(
        ".git/strict-weave/rebase-state.json",
        &serde_json::to_string(&state).unwrap(),
    );
    let output = repo.tool_code(&["rebase", "--continue"], 129);
    assert!(String::from_utf8_lossy(&output.stderr).contains("应用未完整结束"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    repo.tool_code(&["rebase", "--abort"], 0);
}

#[test]
fn failed_later_analysis_can_be_retried_without_losing_completed_replays() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.write("second.go", "package second\n");
    repo.commit("second replay");
    repo.tool_code(&["rebase", "feature", "--zdiff3", "--explain-reasons"], 1);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.git(&["config", "core.autocrlf", "true"]);
    let output = repo.tool_code(&["rebase", "--continue"], 129);
    assert!(String::from_utf8_lossy(&output.stderr).contains("core.autocrlf"));
    let state = repo.rebase_state();
    assert_eq!(state["next"], 1);
    assert_eq!(state["phase"]["kind"], "ready");
    assert_eq!(
        state["options"],
        serde_json::json!({"zdiff3": true, "detailed": true})
    );
    let completed = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["config", "core.autocrlf", "false"]);
    repo.tool_code(&["rebase", "--continue"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), completed);
    assert_eq!(repo.git(&["rev-list", "--count", "feature..HEAD"]), "2");
}
