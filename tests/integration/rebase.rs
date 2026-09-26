use super::*;
#[test]
fn rebase_single_commit_preserves_strict_state_for_continue() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("calc.go", THEIRS);
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", OURS);
    repo.commit("ours");
    let out = repo.tool(&["rebase", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    let continued = repo.tool(&["rebase", "--continue"]);
    assert!(
        continued.status.success(),
        "{}",
        String::from_utf8_lossy(&continued.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
}

#[test]
fn rebase_replays_multiple_commits_and_reanalyzes_after_continue() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.write("calc.go", OURS);
    repo.commit("upstream");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", THEIRS);
    repo.commit("feature one");
    repo.write("calc.go", BASE);
    repo.write("second.go", "package p\n\nfunc second() int { return 2 }\n");
    repo.commit("feature two");

    let first = repo.tool(&["rebase", "upstream"]);
    assert_eq!(
        first.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(String::from_utf8_lossy(&first.stderr).contains("ENTITY_CONFLICT"));
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);

    let second = repo.tool(&["rebase", "--continue"]);
    assert_eq!(
        second.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stderr).contains("ENTITY_CONFLICT"));
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    repo.write("calc.go", THEIRS);
    repo.git(&["add", "calc.go"]);

    let finished = repo.tool(&["rebase", "--continue"]);
    assert_eq!(
        finished.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&finished.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert_eq!(repo.git(&["rev-list", "--count", "upstream..HEAD"]), "2");
    assert_eq!(
        repo.git(&["rev-parse", "HEAD~2"]),
        repo.git(&["rev-parse", "upstream"])
    );
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
}

#[test]
fn rebase_abort_restores_original_branch_and_worktree() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("calc.go", THEIRS);
    repo.commit("feature");
    let original = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", OURS);
    repo.commit("ours");
    let before = repo.git(&["rev-parse", "HEAD"]);

    let started = repo.tool(&["rebase", "feature"]);
    assert_eq!(started.status.code(), Some(1));
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    let aborted = repo.tool(&["rebase", "--abort"]);
    assert_eq!(
        aborted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&aborted.stderr)
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        OURS
    );
    assert_eq!(repo.git(&["rev-parse", "feature"]), original);
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
}

#[test]
fn clean_single_commit_rebase_updates_branch_without_rebase_state() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("feature.go", "package p\n");
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main.go", "package p\n");
    repo.commit("main");
    let out = repo.tool(&["rebase", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert_eq!(repo.git(&["rev-list", "--count", "feature..HEAD"]), "1");
    assert!(repo.path().join("main.go").exists());
}

#[test]
fn rebase_plan_is_read_only_and_apply_reuses_it() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("feature.go", "package p\n");
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main.go", "package p\n");
    repo.commit("main");
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("rebase-plan.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["rebase", "feature", "--plan", "-o", plan_text]);
    assert_eq!(planned.status.code(), Some(0));
    assert!(plan.is_file());
    assert!(!repo.path().join(".git/rebase-merge").exists());
    let applied = repo.tool(&["rebase", "feature", "--apply", plan_text]);
    assert_eq!(
        applied.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}
