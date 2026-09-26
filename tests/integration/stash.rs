use super::*;

#[test]
fn failed_native_stash_apply_preserves_stash_and_hidden_worktree_changes() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        for strict_conflict in [false, true] {
            let repo = Repo::new(BASE);
            repo.write("calc.go", THEIRS);
            repo.git(&["stash", "push", "-qm", "work"]);
            if strict_conflict {
                repo.write("calc.go", OURS);
                repo.commit("ours");
            }
            // Git status hides these local edits, but stash apply rejects them.
            // Exit 1 without unmerged entries is an error, not a conflict.
            repo.git(&["update-index", flag, "calc.go"]);
            let local = if strict_conflict { BASE } else { OURS };
            repo.write("calc.go", local);
            assert_eq!(repo.git(&["status", "--porcelain"]), "");
            let head = repo.git(&["rev-parse", "HEAD"]);
            let stash = repo.git(&["rev-parse", "refs/stash"]);
            let index = repo.git(&["ls-files", "--stage", "-v"]);

            let output = repo.tool_code(&["stash", "pop"], 129);
            assert!(String::from_utf8_lossy(&output.stderr).contains("Git stash apply"));
            assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
            assert_eq!(repo.git(&["rev-parse", "refs/stash"]), stash);
            assert_eq!(repo.git(&["ls-files", "--stage", "-v"]), index);
            assert_eq!(repo.content("calc.go"), local);
            assert!(repo.git(&["ls-files", "-u"]).is_empty());
        }
    }
}

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
