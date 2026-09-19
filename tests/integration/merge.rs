use crate::support::*;
use std::fs;

#[test]
fn independent_entities_merge_via_real_driver() {
    let r = Repo::new(INDEPENDENT, "calc.rs");
    r.diverge(INDEPENDENT, "calc.rs");
    r.ok(&["merge", "other"]);
    r.assert_file("calc.rs", INDEPENDENT, "output");
    assert_eq!(
        r.git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split_whitespace()
            .count(),
        3
    );
    r.assert_clean();
    assert_eq!(r.report_count(), 1);
}

#[test]
fn disjoint_function_edits_stop_even_though_native_git_merges_cleanly() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.diverge(SIMPLE, "calc.go");
    let before = r.head();
    // Prove that this fixture exercises the strict extension, not a plain
    // line conflict. Native Git uses its text driver for the baseline.
    r.git(&[
        "-c",
        "merge.strict-weave.driver=git merge-file %A %O %B",
        "merge",
        "--no-edit",
        "other",
    ]);
    r.git(&["reset", "--hard", &before]);
    let index = r.index();
    let refs = r.git(&["show-ref"]);
    let out = r.tool(&["merge", "other"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("ENTITY_CONFLICT"));
    assert_eq!(r.head(), before);
    assert_eq!(r.git(&["show-ref"]), refs);
    assert_eq!(r.index(), index);
    r.assert_file("calc.go", SIMPLE, "ours");
    assert!(!r.path().join(".git/MERGE_HEAD").exists());
    r.assert_clean();
}

#[test]
fn equal_blobs_cannot_bypass_preparation() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.git(&["checkout", "-qb", "other"]);
    r.copy(SIMPLE, "ours", "calc.go");
    r.commit("theirs same edit");
    r.git(&["checkout", "-q", "main"]);
    r.copy(SIMPLE, "ours", "calc.go");
    r.commit("ours same edit");
    let before = r.head();
    let out = r.tool(&["merge", "other"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("相同"));
    assert_eq!(r.head(), before);
    r.assert_clean();
}

#[test]
fn cross_file_move_conflict_is_reported_before_merge() {
    let r = Repo::new("moves/move-source.go", "source.go");
    r.git(&["checkout", "-qb", "other"]);
    r.copy("moves/move-source.go", "theirs", "source.go");
    r.copy("moves/move-target.go", "theirs", "target.go");
    r.commit("move function");
    r.git(&["checkout", "-q", "main"]);
    r.copy("moves/move-source.go", "ours", "source.go");
    r.commit("modify function");
    let before = r.head();
    let out = r.tool(&["merge", "other", "--explain-reasons"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("MOVE_CANDIDATE"));
    assert!(stderr(&out).contains("target.go"));
    assert_eq!(r.head(), before);
    assert!(!r.path().join("target.go").exists());
    r.assert_clean();
}

#[test]
fn merge_modes_and_abort_remain_native_git_operations() {
    for mode in ["--ff-only", "--no-ff", "--squash", "--no-commit"] {
        let r = Repo::new(INDEPENDENT, "calc.rs");
        r.git(&["checkout", "-qb", "other"]);
        r.copy(INDEPENDENT, "theirs", "calc.rs");
        r.commit("theirs");
        let theirs = r.head();
        r.git(&["checkout", "-q", "main"]);
        let before = r.head();
        let mut args = vec!["merge", "other", mode];
        if mode == "--no-commit" {
            args.push("--no-ff");
        }
        r.ok(&args);
        r.assert_file("calc.rs", INDEPENDENT, "theirs");
        match mode {
            "--ff-only" => assert_eq!(r.head(), theirs),
            "--no-ff" => assert_eq!(r.git(&["rev-parse", "HEAD^2"]), theirs),
            "--squash" => {
                assert_eq!(r.head(), before);
                assert!(!r.path().join(".git/MERGE_HEAD").exists());
            }
            _ => {
                assert_eq!(r.head(), before);
                r.ok(&["merge", "--abort"]);
                r.assert_file("calc.rs", INDEPENDENT, "base");
                r.assert_clean();
            }
        }
    }
}

#[test]
fn dirty_and_unsupported_modes_are_rejected_without_mutation() {
    let r = Repo::new(SIMPLE, "calc.go");
    let before = r.head();
    fs::write(r.path().join("untracked"), "local work").unwrap();
    assert_code(&r.tool(&["merge", "main"]), 129);
    for args in [
        vec!["merge", "main", "other"],
        vec!["merge", "main", "-s", "ours"],
        vec!["rebase", "--autostash"],
        vec!["rebase", "--apply"],
    ] {
        assert_code(&r.tool(&args), 2);
    }
    assert_eq!(r.head(), before);
    assert_eq!(
        fs::read_to_string(r.path().join("untracked")).unwrap(),
        "local work"
    );
}

#[test]
fn line_conflicts_are_never_converted_into_success() {
    let case = "entities/opposed.go";
    let r = Repo::new(case, "calc.go");
    r.diverge(case, "calc.go");
    let before = r.head();
    let baseline = r
        .command(
            "git",
            &[
                "-c",
                "merge.strict-weave.driver=git merge-file %A %O %B",
                "merge",
                "--no-edit",
                "other",
            ],
        )
        .output()
        .unwrap();
    assert_code(&baseline, 1);
    assert!(!r.git(&["ls-files", "-u"]).is_empty());
    r.git(&["merge", "--abort"]);
    let out = r.tool(&["merge", "other"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("LINE_CONFLICT"));
    assert_eq!(r.head(), before);
    r.assert_clean();
}

#[test]
fn branch_merge_options_cannot_override_requested_strategy() {
    let r = Repo::new(INDEPENDENT, "calc.rs");
    r.diverge(INDEPENDENT, "calc.rs");
    r.git(&["config", "branch.main.mergeOptions", "-s ours -Xours"]);
    r.ok(&["merge", "other"]);
    r.assert_file("calc.rs", INDEPENDENT, "output");
    assert_eq!(
        r.git(&["config", "branch.main.mergeOptions"]),
        "-s ours -Xours"
    );
}

#[test]
fn git_alias_and_linked_worktree_use_the_correct_git_directory() {
    let r = Repo::new(INDEPENDENT, "calc.rs");
    r.diverge(INDEPENDENT, "calc.rs");
    let linked = tempfile::Builder::new()
        .prefix("linked weave's-")
        .tempdir_in("/tmp")
        .unwrap();
    r.git(&[
        "worktree",
        "add",
        "-qb",
        "linked",
        linked.path().to_str().unwrap(),
        "main",
    ]);
    let alias = format!("!'{}' merge", TOOL.replace('\'', "'\\''"));
    r.git(&["config", "alias.strict-merge", &alias]);
    let out = r
        .command("git", &["strict-merge", "other"])
        .current_dir(linked.path())
        .output()
        .unwrap();
    assert_code(&out, 0);
    assert_eq!(
        fs::read_to_string(linked.path().join("calc.rs")).unwrap(),
        crate::common::text(INDEPENDENT, "output")
    );
    r.assert_file("calc.rs", INDEPENDENT, "ours");
    assert!(!r.path().join(".git/strict-weave").exists());
    assert!(r.path().join(".git/worktrees").is_dir());
}

#[test]
fn multiple_merge_bases_are_rejected_before_mutation() {
    let r = Repo::new(SIMPLE, "calc.go");
    let base = r.head();
    r.copy(INDEPENDENT, "base", "left.rs");
    r.commit("left");
    r.git(&["branch", "left-original"]);
    r.git(&["checkout", "-qb", "right", &base]);
    r.copy(INDEPENDENT, "base", "right.rs");
    r.commit("right");
    r.git(&["branch", "right-original"]);
    r.git(&["merge", "--no-edit", "left-original"]);
    r.git(&["checkout", "-q", "main"]);
    r.git(&["merge", "--no-edit", "right-original"]);
    assert_eq!(
        r.git(&["merge-base", "--all", "main", "right"])
            .lines()
            .count(),
        2
    );
    let before = r.head();
    assert_code(&r.tool(&["merge", "right"]), 129);
    assert_eq!(r.head(), before);
    r.assert_clean();
}

#[test]
fn invoking_from_subdirectory_still_checks_conflicts_in_other_directories() {
    let r = Repo::new(SIMPLE, "outside.go");
    r.copy(INDEPENDENT, "base", "src/inside.rs");
    r.commit("inside");
    r.diverge(SIMPLE, "outside.go");
    let before = r.head();
    let out = r
        .command(TOOL, &["merge", "other"])
        .current_dir(r.path().join("src"))
        .output()
        .unwrap();
    assert_code(&out, 1);
    assert!(stderr(&out).contains("outside.go"));
    assert_eq!(r.head(), before);
    r.assert_clean();
}
