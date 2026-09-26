use super::*;

#[test]
fn continue_inherits_zdiff3_and_renders_the_shared_fixture_output() {
    const ADJACENT_BASE: &str = include_str!("../fixtures/layout/adjacent.ts.base");
    const ADJACENT_OURS: &str = include_str!("../fixtures/layout/adjacent.ts.ours");
    const ADJACENT_THEIRS: &str = include_str!("../fixtures/layout/adjacent.ts.theirs");
    const ADJACENT_OUTPUT: &str = include_str!("../fixtures/layout/adjacent.ts.output-zdiff3");
    let repo = Repo::new(BASE);
    repo.write("adjacent.ts", ADJACENT_BASE);
    repo.commit("adjacent base");
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.write("calc.go", OURS);
    repo.write("adjacent.ts", ADJACENT_OURS);
    repo.commit("upstream");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", THEIRS);
    repo.commit("first replay");
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("adjacent.ts", ADJACENT_THEIRS);
    repo.commit("second replay");
    let theirs = repo.git(&["rev-parse", "HEAD"]);
    repo.tool_code(&["rebase", "upstream", "--zdiff3"], 1);
    repo.write("calc.go", OURS);
    repo.git(&["add", "calc.go"]);
    repo.tool_code(&["rebase", "--continue"], 1);
    let ours = repo.git(&["rev-parse", "HEAD"]);
    let expected = ADJACENT_OUTPUT
        .replace("feature/ours", &ours)
        .replace("base-commit", &base)
        .replace("feature/theirs", &theirs);
    assert_eq!(repo.content("adjacent.ts"), expected);
    repo.tool_code(&["rebase", "--abort"], 0);
}

#[test]
fn later_replay_analyzes_cross_file_moves_using_the_manual_resolution() {
    let repo = Repo::new(BASE);
    repo.copy_fixture_tree("multi-file/modify-vs-move.base");
    repo.commit("source before move");
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.copy_fixture_tree("multi-file/modify-vs-move.ours");
    repo.write("calc.go", OURS);
    repo.commit("upstream modifications");
    let upstream = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", THEIRS);
    repo.commit("first replay");
    let first_original = repo.git(&["rev-parse", "HEAD"]);
    repo.copy_fixture_tree("multi-file/modify-vs-move.theirs");
    repo.commit("second replay moves calculate");
    let original = repo.git(&["rev-parse", "HEAD"]);

    repo.tool_code(&["rebase", "upstream"], 1);
    let previous_reports = repo.plans();
    assert_eq!(repo.git(&["rev-parse", "main"]), original);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), upstream);
    // Resolve calc.go, and remove retained() using another existing fixture.
    // The next replay must see this manual result, not upstream's source tree.
    repo.write("calc.go", BASE);
    const RESOLVED: &str = include_str!("../fixtures/moves/calculate.go.ours");
    repo.write("src/source.go", RESOLVED);
    repo.git(&["add", "."]);
    let output = repo.tool_code(&["rebase", "--continue"], 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("GLOBAL_MODIFY_VS_MOVE"));
    let replayed = repo.git(&["rev-parse", "HEAD"]);
    assert_ne!(replayed, upstream);
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]), upstream);
    assert_eq!(repo.blob("HEAD:src/source.go"), RESOLVED.as_bytes());
    assert_eq!(repo.blob("HEAD:calc.go"), BASE.as_bytes());
    assert_eq!(
        repo.blob(":1:src/source.go"),
        repo.blob(&format!("{first_original}:src/source.go"))
    );
    assert_eq!(repo.blob(":2:src/source.go"), RESOLVED.as_bytes());
    assert_eq!(
        repo.blob(":3:src/source.go"),
        repo.blob(&format!("{original}:src/source.go"))
    );
    assert_eq!(repo.blob(":2:lib/target.go"), b"");
    assert_eq!(
        repo.blob(":3:lib/target.go"),
        repo.blob(&format!("{original}:lib/target.go"))
    );
    assert!(repo
        .content("lib/target.go")
        .contains("GLOBAL_MODIFY_VS_MOVE"));
    assert!(repo.content("src/source.go").contains("return 2"));

    let reports = repo.plans();
    let fresh: Vec<_> = reports
        .iter()
        .filter(|(path, _)| !previous_reports.contains_key(*path))
        .collect();
    assert_eq!(fresh.len(), 1);
    let plan: serde_json::Value = serde_json::from_slice(fresh[0].1).unwrap();
    assert_eq!(
        plan["revisions"],
        serde_json::json!([first_original, replayed, original])
    );
    assert_eq!(
        plan["report"]["trees"],
        serde_json::json!([
            repo.tree(&first_original),
            repo.tree(&replayed),
            repo.tree(&original)
        ])
    );
    assert_eq!(plan["report"]["move_candidates"][0]["opposite"], "modified");

    // Aborting after one successful replay must still restore the initial tip.
    repo.tool_code(&["rebase", "--abort"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), original);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert_eq!(
        repo.blob("HEAD:src/source.go"),
        repo.content("src/source.go").as_bytes()
    );
}

#[test]
fn clean_multi_commit_rebase_preserves_order_messages_and_authors() {
    let repo = Repo::new(BASE);
    let metadata = |revision: &str| {
        let output = repo.command(
            "git",
            &["show", "-s", "--format=%an%x00%ae%x00%aI%x00%B", revision],
        );
        assert!(output.status.success());
        output.stdout
    };
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.write("upstream.go", "package upstream\n");
    repo.commit("upstream");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("one.go", "package one\n");
    repo.commit("first\n\nfirst body");
    let first = metadata("HEAD");
    repo.write("two.go", "package two\n");
    repo.commit("second\n\nsecond body");
    let second = metadata("HEAD");
    repo.tool_code(&["rebase", "upstream"], 0);
    assert_eq!(metadata("HEAD^"), first);
    assert_eq!(metadata("HEAD"), second);
    assert_eq!(repo.git(&["rev-list", "--count", "upstream..HEAD"]), "2");
    assert_eq!(
        repo.git(&["rev-parse", "HEAD~2"]),
        repo.git(&["rev-parse", "upstream"])
    );
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert_eq!(repo.plans().len(), 2);
}

#[test]
fn rebase_preview_analyzes_clean_steps_stops_at_conflict_and_binds_its_reports() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.write("calc.go", OURS);
    repo.commit("upstream");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("one.go", "package one\n");
    repo.commit("first clean replay");
    repo.write("calc.go", THEIRS);
    repo.commit("second conflicting replay");
    repo.write("three.go", "package three\n");
    repo.commit("third pending replay");
    let before = repo.git(&["rev-parse", "HEAD"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preview.json");
    let args = ["rebase", "upstream", "--plan", "-o", path.to_str().unwrap()];
    let output = repo.tool_code(&args, 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("后续 1 个 commit 尚未分析"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(repo.content("calc.go"), THEIRS);
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
    let bytes = fs::read(&path).unwrap();
    let mut plan: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(plan["steps"].as_array().unwrap().len(), 2);
    assert!(plan["steps"][0]["result_tree"].is_string());
    assert!(plan["steps"][1]["result_tree"].is_null());
    assert_eq!(plan["pending"], serde_json::json!([before]));
    assert!(
        !plan["steps"][1]["analysis"]["report"]["files"]["calc.go"]["reasons"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    plan["steps"][1]["analysis"]["report"]["files"]["calc.go"]["reasons"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    repo.tool_code(
        &["rebase", "upstream", "--apply", path.to_str().unwrap()],
        129,
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    fs::write(&path, bytes).unwrap();
    repo.tool_code(
        &["rebase", "upstream", "--apply", path.to_str().unwrap()],
        1,
    );
    assert_eq!(repo.rebase_state()["next"], 1);
    repo.tool_code(&["rebase", "--abort"], 0);
    // A correct report also expires when its target advances.
    repo.git(&["checkout", "-q", "upstream"]);
    repo.write("advanced.go", "package advanced\n");
    repo.commit("advance upstream");
    repo.git(&["checkout", "-q", "main"]);
    repo.tool_code(
        &["rebase", "upstream", "--apply", path.to_str().unwrap()],
        129,
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
}

#[test]
fn rebase_fast_forward_and_up_to_date_keep_native_history() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "upstream"]);
    repo.write("calc.go", OURS);
    repo.commit("upstream");
    let target = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.tool_code(&["rebase", "upstream"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), target);
    assert_eq!(repo.content("calc.go"), OURS);
    repo.write("main.go", "package p\n");
    repo.commit("already based on upstream");
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.tool_code(&["rebase", "upstream"], 0);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert!(!repo
        .path()
        .join(".git/strict-weave/rebase-state.json")
        .exists());
    repo.tool_code(&["rebase", "--continue"], 129);
}
