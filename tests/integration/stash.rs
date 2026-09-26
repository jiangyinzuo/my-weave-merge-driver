use super::*;
#[test]
fn stash_pop_applies_cleanly_and_drops_only_after_success() {
    let repo = Repo::new(BASE);
    repo.write("calc.go", OURS);
    repo.git(&["stash", "push", "-qm", "work"]);
    let out = repo.tool(&["stash", "pop"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        OURS
    );
    assert_eq!(repo.git(&["stash", "list"]), "");
}

#[test]
fn stash_pop_conflict_keeps_stash_and_installs_index_stages() {
    let repo = Repo::new(BASE);
    repo.write("calc.go", THEIRS);
    repo.git(&["stash", "push", "-qm", "work"]);
    repo.write("calc.go", OURS);
    repo.commit("head");
    let out = repo.tool(&["stash", "pop"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    assert!(!repo.git(&["stash", "list"]).is_empty());
}

#[test]
fn stash_pop_plan_is_read_only_and_apply_consumes_it() {
    let repo = Repo::new(BASE);
    repo.write("calc.go", OURS);
    repo.git(&["stash", "push", "-qm", "work"]);
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("stash-plan.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["stash", "pop", "--plan", "-o", plan_text]);
    assert_eq!(planned.status.code(), Some(0));
    assert!(plan.is_file());
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        BASE
    );
    assert!(!repo.git(&["stash", "list"]).is_empty());
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    let applied = repo.tool(&["stash", "pop", "--apply", plan_text]);
    assert_eq!(
        applied.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(repo.git(&["stash", "list"]), "");
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        OURS
    );
}
