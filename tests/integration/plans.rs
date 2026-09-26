use super::*;
#[test]
fn plan_only_is_read_only_and_apply_consumes_the_bound_plan() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("merge-plan.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["merge", "feature", "--plan", "--output", plan_text]);
    assert_eq!(planned.status.code(), Some(1));
    assert!(plan.is_file());
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/MERGE_HEAD").exists());

    let applied = repo.tool(&["merge", "feature", "--apply", plan_text]);
    assert_eq!(
        applied.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
}

#[test]
fn stale_plan_is_rejected_before_git_state_changes() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("stale.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["merge", "feature", "--plan", "--output", plan_text]);
    assert_eq!(planned.status.code(), Some(1));
    repo.write("unrelated.txt", "changed after planning\n");
    repo.commit("advance ours");
    let before = repo.git(&["rev-parse", "HEAD"]);
    let applied = repo.tool(&["merge", "feature", "--apply", plan_text]);
    assert_eq!(applied.status.code(), Some(129));
    assert!(String::from_utf8_lossy(&applied.stderr).contains("分析计划"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert!(repo.git(&["ls-files", "-u"]).is_empty());
}
