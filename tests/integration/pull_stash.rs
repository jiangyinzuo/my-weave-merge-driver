use crate::support::*;
use std::fs;

#[test]
fn pull_fetches_local_remote_then_merges_or_rebases() {
    for mode in [
        "--no-rebase",
        "--rebase",
        "--rebase=interactive",
        "--rebase=merges",
        "--ff-only",
    ] {
        let remote = Repo::new(INDEPENDENT, "calc.rs");
        let local = Repo::new(INDEPENDENT, "calc.rs");
        local.git(&["remote", "add", "origin", remote.path().to_str().unwrap()]);
        local.git(&["fetch", "-q", "origin", "main"]);
        local.git(&["reset", "--hard", "FETCH_HEAD"]);
        local.git(&["branch", "--set-upstream-to=origin/main"]);
        remote.copy(INDEPENDENT, "theirs", "calc.rs");
        remote.commit("remote edit");
        if mode != "--ff-only" {
            local.copy(INDEPENDENT, "ours", "calc.rs");
            local.commit("local edit");
        }
        local.ok(&["pull", mode]);
        local.assert_file(
            "calc.rs",
            INDEPENDENT,
            if mode == "--ff-only" {
                "theirs"
            } else {
                "output"
            },
        );
        assert_eq!(local.git(&["rev-parse", "FETCH_HEAD"]), remote.head());
        local.assert_clean();
    }
}

#[test]
fn pull_conflict_fetches_but_does_not_merge_or_change_local_files() {
    let remote = Repo::new(SIMPLE, "calc.go");
    let local = Repo::new(SIMPLE, "calc.go");
    local.git(&["remote", "add", "origin", remote.path().to_str().unwrap()]);
    local.git(&["fetch", "-q", "origin", "main"]);
    local.git(&["reset", "--hard", "FETCH_HEAD"]);
    remote.copy(SIMPLE, "theirs", "calc.go");
    remote.commit("remote edit");
    local.copy(SIMPLE, "ours", "calc.go");
    local.commit("local edit");
    let before = local.head();
    assert_code(&local.tool(&["pull", "--no-rebase", "origin", "main"]), 1);
    assert_eq!(local.head(), before);
    assert_eq!(local.git(&["rev-parse", "FETCH_HEAD"]), remote.head());
    local.assert_file("calc.go", SIMPLE, "ours");
    local.assert_clean();
}

#[test]
fn stash_apply_and_pop_merge_independent_entities_and_drop_only_on_success() {
    for action in ["apply", "pop"] {
        let r = Repo::new(INDEPENDENT, "calc.rs");
        r.copy(INDEPENDENT, "theirs", "calc.rs");
        r.git(&["stash", "push", "-qm", "saved edit"]);
        let stash = r.git(&["rev-parse", "refs/stash"]);
        r.copy(INDEPENDENT, "ours", "calc.rs");
        r.commit("upstream edit");
        let before = r.head();
        r.ok(&["stash", action]);
        assert_eq!(r.head(), before);
        r.assert_file("calc.rs", INDEPENDENT, "output");
        assert_eq!(r.git(&["ls-files", "-u"]), "");
        if action == "apply" {
            assert_eq!(r.git(&["rev-parse", "refs/stash"]), stash);
        } else {
            assert_eq!(r.git(&["stash", "list"]), "");
        }
    }
}

#[test]
fn stash_conflict_preserves_stash_index_and_dirty_worktree() {
    for staged in [false, true] {
        let r = Repo::new(SIMPLE, "calc.go");
        r.copy(SIMPLE, "theirs", "calc.go");
        r.git(&["stash", "push", "-qm", "saved edit"]);
        let stash = r.git(&["rev-parse", "refs/stash"]);
        r.copy(SIMPLE, "ours", "calc.go");
        if staged {
            r.git(&["add", "calc.go"]);
        }
        let before = r.head();
        let index_tree = r.git(&["write-tree"]);
        assert_code(&r.tool(&["stash", "pop", "--index"]), 1);
        assert_eq!(r.head(), before);
        assert_eq!(r.git(&["write-tree"]), index_tree);
        assert_eq!(r.git(&["rev-parse", "refs/stash"]), stash);
        r.assert_file("calc.go", SIMPLE, "ours");
        assert_eq!(r.git(&["ls-files", "-u"]), "");
    }
}

#[test]
fn stash_index_restoration_and_untracked_parent_are_preserved() {
    let r = Repo::new(INDEPENDENT, "calc.rs");
    r.copy(INDEPENDENT, "ours", "calc.rs");
    r.git(&["add", "calc.rs"]);
    let staged_tree = r.git(&["write-tree"]);
    r.copy(INDEPENDENT, "output", "calc.rs");
    r.copy(SIMPLE, "base", "untracked.go");
    r.git(&[
        "stash",
        "push",
        "-qu",
        "-m",
        "staged unstaged and untracked",
    ]);
    r.ok(&["stash", "pop", "--index"]);
    assert_eq!(r.git(&["write-tree"]), staged_tree);
    r.assert_file("calc.rs", INDEPENDENT, "output");
    r.assert_file("untracked.go", SIMPLE, "base");
    assert_eq!(r.git(&["stash", "list"]), "");
    assert_eq!(
        r.git(&["ls-files", "--others", "--exclude-standard"]),
        "untracked.go"
    );
}

#[test]
fn stash_move_to_untracked_file_is_included_in_analysis() {
    let r = Repo::new("moves/move-source.go", "source.go");
    r.copy("moves/move-source.go", "theirs", "source.go");
    r.copy("moves/move-target.go", "theirs", "target.go");
    r.git(&["stash", "push", "-qu"]);
    r.copy("moves/move-source.go", "ours", "source.go");
    r.commit("modify source");
    let out = r.tool(&["stash", "pop"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("MOVE_CANDIDATE"));
    assert!(stderr(&out).contains("target.go"));
    assert!(!r.path().join("target.go").exists());
    assert!(!r.git(&["stash", "list"]).is_empty());
    r.assert_clean();
}

#[test]
fn stash_rejects_current_untracked_files_without_overwriting_them() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.copy(SIMPLE, "ours", "calc.go");
    r.git(&["stash", "push", "-q"]);
    fs::write(r.path().join("untracked"), "keep").unwrap();
    assert_code(&r.tool(&["stash", "pop"]), 129);
    assert_eq!(
        fs::read_to_string(r.path().join("untracked")).unwrap(),
        "keep"
    );
    assert!(!r.git(&["stash", "list"]).is_empty());
}

#[test]
fn stash_index_can_be_restored_on_top_of_independent_committed_edits() {
    let r = Repo::new(INDEPENDENT, "calc.rs");
    r.copy(INDEPENDENT, "theirs", "calc.rs");
    r.git(&["add", "calc.rs"]);
    r.git(&["stash", "push", "-q"]);
    r.copy(INDEPENDENT, "ours", "calc.rs");
    r.commit("independent upstream edit");
    r.ok(&["stash", "apply", "--index"]);
    r.assert_file("calc.rs", INDEPENDENT, "output");
    assert_eq!(r.git(&["diff", "--name-only"]), "");
    assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "calc.rs");
}

#[test]
fn stash_operates_from_a_subdirectory_without_losing_paths() {
    let r = Repo::new(INDEPENDENT, "src/calc.rs");
    r.copy(INDEPENDENT, "theirs", "src/calc.rs");
    r.git(&["stash", "push", "-q"]);
    r.copy(INDEPENDENT, "ours", "src/calc.rs");
    r.commit("upstream");
    let out = r
        .command(TOOL, &["stash", "pop"])
        .current_dir(r.path().join("src"))
        .output()
        .unwrap();
    assert_code(&out, 0);
    r.assert_file("src/calc.rs", INDEPENDENT, "output");
}

#[test]
fn stash_subdirectory_analysis_includes_untracked_files_outside_that_directory() {
    let r = Repo::new(INDEPENDENT, "src/calc.rs");
    r.copy(INDEPENDENT, "theirs", "src/calc.rs");
    r.copy(SIMPLE, "base", "outside.go");
    r.git(&["stash", "push", "-qu"]);
    let out = r
        .command(TOOL, &["stash", "pop"])
        .current_dir(r.path().join("src"))
        .output()
        .unwrap();
    assert_code(&out, 0);
    r.assert_file("src/calc.rs", INDEPENDENT, "theirs");
    r.assert_file("outside.go", SIMPLE, "base");
    r.git(&["stash", "push", "-qu"]);
    fs::write(r.path().join("outside"), "keep").unwrap();
    let out = r
        .command(TOOL, &["stash", "pop"])
        .current_dir(r.path().join("src"))
        .output()
        .unwrap();
    assert_code(&out, 129);
    assert!(!r.git(&["stash", "list"]).is_empty());
}
