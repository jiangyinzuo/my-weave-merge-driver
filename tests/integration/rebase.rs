use crate::support::*;
use std::fs;

fn series() -> Repo {
    let r = Repo::new(SIMPLE, "calc.go");
    r.git(&["branch", "topic"]);
    r.copy(INDEPENDENT, "base", "upstream.rs");
    r.commit("upstream");
    r.git(&["checkout", "-q", "topic"]);
    r.copy(INDEPENDENT, "base", "first.rs");
    r.commit("first");
    r.copy(INDEPENDENT, "base", "second.rs");
    r.commit("second");
    r
}

#[test]
fn every_replayed_commit_gets_its_own_report() {
    let r = series();
    r.ok(&["rebase", "main"]);
    assert_eq!(
        r.git(&["log", "--format=%s", "main..HEAD"]),
        "second\nfirst"
    );
    assert_eq!(r.report_count(), 2);
    assert_eq!(
        r.git(&["rev-parse", "HEAD~2"]),
        r.git(&["rev-parse", "main"])
    );
    for path in ["first.rs", "second.rs", "upstream.rs"] {
        r.assert_file(path, INDEPENDENT, "base");
    }
    assert!(!r.path().join(".git/strict-weave/rebase.json").exists());
    r.assert_clean();
}

#[test]
fn later_conflict_stops_before_pick_continue_rechecks_and_skip_drops_only_that_pick() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.git(&["branch", "topic"]);
    r.copy(SIMPLE, "ours", "calc.go");
    r.commit("upstream edit");
    r.git(&["checkout", "-q", "topic"]);
    r.copy(INDEPENDENT, "base", "first.rs");
    r.commit("first");
    r.copy(SIMPLE, "theirs", "calc.go");
    r.commit("conflicting second");
    r.copy(INDEPENDENT, "base", "third.rs");
    r.commit("third");
    assert_code(&r.tool(&["rebase", "main"]), 1);
    assert_eq!(r.git(&["log", "-1", "--format=%s"]), "first");
    r.assert_file("calc.go", SIMPLE, "ours");
    assert!(!r.path().join("third.rs").exists());
    assert_eq!(r.git(&["ls-files", "-u"]), "");
    let stopped = r.head();
    assert_code(&r.tool(&["rebase", "--continue"]), 1);
    assert_eq!(r.head(), stopped);
    r.ok(&["rebase", "--skip"]);
    assert_eq!(r.git(&["log", "--format=%s", "main..HEAD"]), "third\nfirst");
    r.assert_file("calc.go", SIMPLE, "ours");
    r.assert_file("third.rs", INDEPENDENT, "base");
    r.assert_clean();
}

#[test]
fn abort_restores_original_branch_tree_and_commit() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.diverge(SIMPLE, "calc.go");
    let before = r.head();
    assert_code(&r.tool(&["rebase", "other"]), 1);
    r.ok(&["rebase", "--abort"]);
    assert_eq!(r.head(), before);
    assert_eq!(r.git(&["branch", "--show-current"]), "main");
    r.assert_file("calc.go", SIMPLE, "ours");
    r.assert_clean();
}

#[test]
fn interactive_squash_and_fixup_preserve_content_and_commit_grouping() {
    for action in ["squash", "fixup", "fixup -C"] {
        let r = series();
        let script = format!("sed -i '2s/^pick /{action} /'");
        let out = r
            .command(TOOL, &["rebase", "-i", "main"])
            .env("GIT_SEQUENCE_EDITOR", script)
            .output()
            .unwrap();
        assert_code(&out, 0);
        assert_eq!(r.git(&["rev-list", "--count", "main..HEAD"]), "1");
        assert_eq!(r.report_count(), 2);
        r.assert_file("first.rs", INDEPENDENT, "base");
        r.assert_file("second.rs", INDEPENDENT, "base");
        r.assert_clean();
    }
}

#[test]
fn interactive_reorder_and_drop_follow_edited_todo() {
    let r = series();
    let out = r
        .command(TOOL, &["rebase", "-i", "main"])
        .env("GIT_SEQUENCE_EDITOR", "sed -i '1{h;d;};2{G;}'")
        .output()
        .unwrap();
    assert_code(&out, 0);
    assert_eq!(
        r.git(&["log", "--format=%s", "main..HEAD"]),
        "first\nsecond"
    );
    r.assert_clean();
    let r = series();
    let out = r
        .command(TOOL, &["rebase", "-i", "main"])
        .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /drop /'")
        .output()
        .unwrap();
    assert_code(&out, 0);
    assert_eq!(r.git(&["log", "--format=%s", "main..HEAD"]), "second");
    assert!(!r.path().join("first.rs").exists());
    assert_eq!(r.report_count(), 1);
}

#[test]
fn interactive_edit_and_edit_todo_continue_through_native_sequencer() {
    let r = series();
    let out = r
        .command(TOOL, &["rebase", "-i", "main"])
        .env("GIT_SEQUENCE_EDITOR", "sed -i '1s/^pick /edit /'")
        .output()
        .unwrap();
    assert_code(&out, 0);
    assert!(r.path().join(".git/rebase-merge").exists());
    assert_eq!(r.git(&["log", "-1", "--format=%s"]), "first");
    // --edit-todo must use the editor from THIS invocation, including for
    // originally non-interactive rebases.
    let out = r
        .command(TOOL, &["rebase", "--edit-todo"])
        .env("GIT_SEQUENCE_EDITOR", "sed -i 's/^pick /drop /'")
        .output()
        .unwrap();
    assert_code(&out, 0);
    r.ok(&["rebase", "--continue"]);
    assert!(!r.path().join("second.rs").exists());
    assert_eq!(r.git(&["log", "--format=%s", "main..HEAD"]), "first");
}

#[test]
fn root_onto_and_explicit_branch_are_supported() {
    let r = series();
    r.git(&["checkout", "-q", "main"]);
    r.ok(&["rebase", "main", "topic", "--onto", "main"]);
    assert_eq!(r.git(&["branch", "--show-current"]), "topic");
    assert_eq!(
        r.git(&["rev-parse", "HEAD~2"]),
        r.git(&["rev-parse", "main"])
    );
    let tree = r.git(&["rev-parse", "HEAD^{tree}"]);
    r.ok(&["rebase", "--root"]);
    assert_eq!(r.git(&["rev-parse", "HEAD^{tree}"]), tree);
    r.assert_clean();
}

#[test]
fn rebase_merges_recreates_two_parent_commit() {
    let r = series();
    r.git(&["checkout", "-qb", "side", "HEAD~1"]);
    r.copy(INDEPENDENT, "base", "side.rs");
    r.commit("side");
    r.git(&["checkout", "-q", "topic"]);
    r.git(&["merge", "--no-ff", "--no-edit", "side"]);
    r.ok(&["rebase", "--rebase-merges", "main"]);
    assert_eq!(
        r.git(&["rev-list", "--parents", "-n1", "HEAD"])
            .split_whitespace()
            .count(),
        3
    );
    for p in ["first.rs", "second.rs", "side.rs", "upstream.rs"] {
        r.assert_file(p, INDEPENDENT, "base");
    }
    assert_eq!(r.report_count(), 4);
    r.assert_clean();
}

#[test]
fn failing_sequence_editor_never_applies_commits() {
    let r = series();
    let before = r.head();
    let out = r
        .command(TOOL, &["rebase", "-i", "main"])
        .env("GIT_SEQUENCE_EDITOR", "false")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(r.head(), before);
    assert!(!r.path().join(".git/rebase-merge").exists());
    assert!(!r.path().join(".git/strict-weave/rebase.json").exists());
    assert_eq!(
        fs::read_to_string(r.path().join("second.rs")).unwrap(),
        crate::common::text(INDEPENDENT, "base")
    );
}

#[test]
fn rebase_merges_checks_the_recreated_merge_itself() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.git(&["branch", "topic"]);
    r.git(&["checkout", "-qb", "side"]);
    r.copy(SIMPLE, "theirs", "calc.go");
    r.commit("side edit");
    r.git(&["checkout", "-q", "topic"]);
    r.copy(SIMPLE, "ours", "calc.go");
    r.commit("topic edit");
    // Original merge was accepted by the line driver, but replay must still
    // apply our strict policy when the sequencer recreates that merge.
    r.git(&[
        "-c",
        "merge.strict-weave.driver=git merge-file %A %O %B",
        "merge",
        "--no-edit",
        "side",
    ]);
    let original = r.head();
    r.git(&["checkout", "-q", "main"]);
    r.copy(INDEPENDENT, "base", "upstream.rs");
    r.commit("upstream");
    r.git(&["checkout", "-q", "topic"]);
    let out = r.tool(&["rebase", "--rebase-merges", "main"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("ENTITY_CONFLICT"));
    assert_eq!(r.report_count(), 3);
    r.assert_file("calc.go", SIMPLE, "ours");
    r.ok(&["rebase", "--abort"]);
    assert_eq!(r.head(), original);
    r.assert_clean();
}

#[test]
fn noninteractive_rebase_can_edit_todo_after_a_blocked_step() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.diverge(SIMPLE, "calc.go");
    assert_code(&r.tool(&["rebase", "other"]), 1);
    let out = r
        .command(TOOL, &["rebase", "--edit-todo"])
        .env("GIT_SEQUENCE_EDITOR", "sed -i 's/^pick /drop /'")
        .output()
        .unwrap();
    assert_code(&out, 0);
    r.ok(&["rebase", "--continue"]);
    assert_eq!(r.head(), r.git(&["rev-parse", "other"]));
    r.assert_clean();
}

#[test]
fn invalid_remaining_todo_does_not_consume_a_blocked_step() {
    let r = Repo::new(SIMPLE, "calc.go");
    r.diverge(SIMPLE, "calc.go");
    assert_code(&r.tool(&["rebase", "other"]), 1);
    let before = r.head();
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(r.path().join(".git/strict-weave/rebase.json")).unwrap())
            .unwrap();
    let blocked = std::path::Path::new(state["directory"].as_str().unwrap()).join("blocked");
    let instruction = fs::read(&blocked).unwrap();
    let path = r.path().join(".git/rebase-merge/git-rebase-todo");
    let original = fs::read_to_string(&path).unwrap();
    // A real target change is not Git's harmless short-hash expansion.
    let pick = original
        .lines()
        .find(|line| line.starts_with("pick "))
        .unwrap();
    let old_hash = pick.split_whitespace().nth(1).unwrap();
    let altered_pick = pick.replacen(old_hash, &before, 1);
    let changed_target = original.replace(pick, &altered_pick);
    fs::write(&path, &changed_target).unwrap();
    let out = r.tool(&["rebase", "--skip"]);
    assert_code(&out, 129);
    assert!(stderr(&out).contains("待办列表已变化"));
    assert_eq!(fs::read_to_string(&path).unwrap(), changed_target);
    assert_eq!(fs::read(&blocked).unwrap(), instruction);
    assert_eq!(r.head(), before);

    let invalid = format!("{original}\nunsupported-command\n");
    fs::write(&path, &invalid).unwrap();

    let out = r.tool(&["rebase", "--skip"]);
    assert_code(&out, 129);
    assert!(
        stderr(&out).contains("不支持的 rebase todo 指令"),
        "{}\ntodo={invalid}",
        stderr(&out)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
    assert_eq!(fs::read(&blocked).unwrap(), instruction);
    assert_eq!(r.head(), before);
    r.assert_file("calc.go", SIMPLE, "theirs");
    assert!(!r.path().join(".git/strict-weave/workflow.lock").exists());

    // Correcting the todo must still permit exactly the original blocked skip.
    fs::write(path, original).unwrap();
    r.ok(&["rebase", "--skip"]);
    assert_eq!(r.head(), r.git(&["rev-parse", "other"]));
    assert!(!blocked.exists());
    r.assert_clean();
}
