use super::*;

#[test]
fn relative_plan_paths_use_callers_directory_and_still_analyze_the_whole_tree() {
    for command in [
        vec!["merge", "feature"],
        vec!["cherry-pick", "feature"],
        vec!["rebase", "feature"],
        vec!["stash", "pop"],
    ] {
        let repo = Repo::new(BASE);
        if command[0] == "stash" {
            repo.write("calc.go", THEIRS);
            repo.git(&["stash", "push", "-qm", "work"]);
            repo.write("calc.go", OURS);
            repo.commit("ours");
        } else {
            repo.diverge(OURS, THEIRS);
        }
        fs::create_dir(repo.path().join("nested")).unwrap();
        repo.write(".git/info/exclude", "plans/\n");
        let original_head = repo.git(&["rev-parse", "HEAD"]);
        let index = fs::read(repo.path().join(".git/index")).unwrap();
        let mut args = command.clone();
        args.extend(["--plan", "-o", "plans/analysis.json"]);
        repo.tool_code_in("nested", &args, 1);
        assert!(repo.path().join("nested/plans/analysis.json").is_file());
        assert!(!repo.path().join("plans/analysis.json").exists());
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), original_head);
        assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
        assert_eq!(repo.content("calc.go"), OURS);

        let mut args = command;
        args.extend(["--apply", "plans/analysis.json"]);
        repo.tool_code_in("nested", &args, 1);
        assert!(repo.content("calc.go").contains("ENTITY_CONFLICT"));
        assert_eq!(repo.blob(":1:calc.go"), BASE.as_bytes());
        assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    }
}

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

#[test]
fn malformed_or_tampered_plan_cannot_remove_conflicts_or_mutate_the_repository() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plan.json");
    let plan_args = ["merge", "feature", "--plan", "-o", path.to_str().unwrap()];
    repo.tool_code(&plan_args, 1);
    let original = fs::read(&path).unwrap();
    let mut tampered: serde_json::Value = serde_json::from_slice(&original).unwrap();
    tampered["report"]["files"]["calc.go"]["reasons"] = serde_json::json!([]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    for bytes in [
        b"{\"report\":".to_vec(),
        serde_json::to_vec(&tampered).unwrap(),
    ] {
        fs::write(&path, &bytes).unwrap();
        repo.tool_code(
            &["merge", "feature", "--apply", path.to_str().unwrap()],
            129,
        );
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
        assert_eq!(repo.content("calc.go"), OURS);
        assert!(!repo.path().join(".git/MERGE_HEAD").exists());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, &original).unwrap();
    repo.tool_code(&plan_args, 129);
    assert_eq!(fs::read(&path).unwrap(), original);
}
