use super::*;

#[test]
fn leftover_clean_plan_cannot_hide_a_later_entity_conflict() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.git(&["checkout", "-qb", "independent"]);
    repo.write("independent.go", "package independent\n");
    repo.commit("independent");
    repo.git(&["checkout", "-q", "main"]);
    repo.tool_code(&["merge", "independent"], 0);
    let stale = repo.plans();
    assert_eq!(stale.len(), 1);
    let old: serde_json::Value = serde_json::from_slice(stale.values().next().unwrap()).unwrap();
    for file in old["report"]["files"].as_object().unwrap().values() {
        assert!(file["reasons"].as_array().unwrap().is_empty());
    }
    repo.tool_code(&["merge", "feature"], 1);
    assert_eq!(repo.blob(":1:calc.go"), BASE.as_bytes());
    assert_eq!(repo.blob(":2:calc.go"), OURS.as_bytes());
    assert_eq!(repo.blob(":3:calc.go"), THEIRS.as_bytes());
    assert!(repo.content("calc.go").contains("ENTITY_CONFLICT"));
    for (path, bytes) in stale {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

/// A whole CLI operation is repeated after abort, not just a report loader.
/// The same starting refs/index must give the same conflict with or without
/// old reports (including a truncated report) in the operation directory.
#[test]
fn leftover_reports_from_other_operations_do_not_affect_merge() {
    let repo = Repo::new(BASE);
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.diverge(OURS, THEIRS);
    let ours = repo.git(&["rev-parse", "HEAD"]);
    let feature = repo.git(&["rev-parse", "feature"]);
    repo.tool_code(&["merge", "feature"], 1);
    repo.git(&["merge", "--abort"]);
    repo.tool_code(&["cherry-pick", &feature], 1);
    repo.git(&["cherry-pick", "--abort"]);
    repo.tool_code(&["rebase", "feature"], 1);
    repo.tool_code(&["rebase", "--abort"], 0);
    let stale = repo.plans();
    assert_eq!(stale.len(), 3);
    repo.write(
        ".git/strict-weave/operation-interrupted/plan.json",
        "{\"report\":",
    );

    repo.git(&["checkout", "-qb", "feature-two", &base]);
    const NEW_THEIRS: &str = include_str!("../fixtures/entities/identical.go.theirs");
    repo.write("calc.go", NEW_THEIRS);
    repo.commit("different target");
    let target = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    let before = repo.plans();
    let result = repo.tool_code(&["merge", "feature-two"], 1);
    assert!(String::from_utf8_lossy(&result.stderr).contains("feature-two"));
    let with_stale = repo.content("calc.go");
    let stages = repo.git(&["ls-files", "--stage"]);
    assert_eq!(repo.blob(":1:calc.go"), BASE.as_bytes());
    assert_eq!(repo.blob(":2:calc.go"), OURS.as_bytes());
    assert_eq!(repo.blob(":3:calc.go"), NEW_THEIRS.as_bytes());
    assert!(with_stale.contains("ENTITY_CONFLICT"));
    assert!(with_stale.contains(">>>>>>> feature-two"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), ours);
    assert_eq!(repo.git(&["rev-parse", "MERGE_HEAD"]), target);

    let after = repo.plans();
    for (path, bytes) in &before {
        assert_eq!(
            after.get(path),
            Some(bytes),
            "old reports must remain unchanged"
        );
    }
    let new: Vec<_> = after
        .iter()
        .filter(|(path, _)| !before.contains_key(*path))
        .collect();
    assert_eq!(new.len(), 1);
    let fresh: serde_json::Value = serde_json::from_slice(new[0].1).unwrap();
    assert_eq!(fresh["target"], "feature-two");
    assert_eq!(fresh["revisions"], serde_json::json!([base, ours, target]));
    assert_eq!(
        fresh["report"]["trees"],
        serde_json::json!([repo.tree(&base), repo.tree(&ours), repo.tree(&target)])
    );

    // Remove only audit reports, restore the exact starting state, then repeat.
    repo.git(&["merge", "--abort"]);
    for path in after.keys() {
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    repo.tool_code(&["merge", "feature-two"], 1);
    assert_eq!(repo.content("calc.go"), with_stale);
    assert_eq!(repo.git(&["ls-files", "--stage"]), stages);
}

#[test]
fn leftover_conflict_plan_does_not_make_a_clean_merge_conflict() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.tool_code(&["merge", "feature"], 1);
    repo.git(&["merge", "--abort"]);
    let stale = repo.plans();
    repo.git(&["checkout", "-qb", "independent", "HEAD~1"]);
    repo.write("independent.go", "package p\n");
    repo.commit("independent");
    let target = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    let original = repo.git(&["rev-parse", "HEAD"]);
    repo.tool_code(&["merge", "independent"], 0);
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert_eq!(repo.git(&["rev-parse", "HEAD^1"]), original);
    assert_eq!(repo.git(&["rev-parse", "HEAD^2"]), target);
    assert_eq!(repo.content("calc.go"), OURS);
    assert_eq!(repo.content("independent.go"), "package p\n");
    for (path, bytes) in stale {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn explicitly_applying_a_previous_targets_plan_is_rejected_without_changes() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.tool_code(&["merge", "feature"], 1);
    repo.git(&["merge", "--abort"]);
    let old = repo.plans().into_keys().next().unwrap();
    repo.git(&["checkout", "-qb", "other", "HEAD~1"]);
    repo.write(
        "calc.go",
        include_str!("../fixtures/entities/identical.go.theirs"),
    );
    repo.commit("other");
    repo.git(&["checkout", "-q", "main"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    repo.tool_code(&["merge", "other", "--apply", old.to_str().unwrap()], 129);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(repo.content("calc.go"), OURS);
    assert!(!repo.path().join(".git/MERGE_HEAD").exists());
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}
