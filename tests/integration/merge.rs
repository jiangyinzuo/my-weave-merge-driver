use super::*;

#[test]
fn clean_divergent_merge_and_cherry_pick_preserve_content_and_history() {
    for operation in ["merge", "cherry-pick"] {
        let case = common::Case::load("entities/independent.rs");
        let repo = Repo::new(BASE);
        repo.write(&case.path, &case.base);
        repo.commit("independent entities");
        repo.git(&["checkout", "-qb", "feature"]);
        repo.write(&case.path, &case.theirs);
        repo.commit("feature change\n\nmessage body");
        repo.git(&[
            "commit",
            "--amend",
            "--no-edit",
            "--author=Original <original@example.invalid>",
        ]);
        let metadata_args = ["show", "-s", "--format=%an%x00%ae%x00%aI%x00%B", "HEAD"];
        let metadata = repo.command("git", &metadata_args).stdout;
        let feature = repo.git(&["rev-parse", "HEAD"]);
        repo.git(&["checkout", "-q", "main"]);
        repo.write(&case.path, &case.ours);
        repo.commit("ours");
        let ours = repo.git(&["rev-parse", "HEAD"]);

        repo.tool_code(&[operation, "feature"], 0);
        assert_eq!(repo.content(&case.path), case.expected());
        assert_eq!(repo.git(&["status", "--porcelain"]), "");
        assert_eq!(repo.git(&["rev-parse", "HEAD^1"]), ours);
        let parents = repo.git(&["show", "-s", "--format=%P", "HEAD"]);
        if operation == "merge" {
            assert_eq!(parents, format!("{ours} {feature}"));
        } else {
            assert_eq!(parents, ours);
            assert_eq!(repo.command("git", &metadata_args).stdout, metadata);
        }
        assert!(!repo.path().join(".git/MERGE_HEAD").exists());
        assert!(!repo.path().join(".git/CHERRY_PICK_HEAD").exists());
    }
}

#[test]
fn native_line_conflicts_and_identical_entity_edits_keep_exact_fixture_output() {
    for (name, native_code, zdiff3) in [
        ("layout/adjacent.ts", 1, true),
        ("entities/identical.go", 0, false),
    ] {
        let case = common::Case::load(name);
        let repo = Repo::new(BASE);
        repo.write(&case.path, &case.base);
        repo.commit("fixture base");
        let base = repo.git(&["rev-parse", "HEAD"]);
        repo.git(&["checkout", "-qb", "feature"]);
        repo.write(&case.path, &case.theirs);
        repo.commit("theirs");
        repo.git(&["checkout", "-q", "main"]);
        repo.write(&case.path, &case.ours);
        repo.commit("ours");
        let ours = repo.git(&["rev-parse", "HEAD"]);
        let native = repo.command("git", &["merge", "--no-ff", "--no-commit", "feature"]);
        assert_eq!(native.status.code(), Some(native_code));
        repo.git(&["merge", "--abort"]);

        let mut args = vec!["merge", "feature"];
        if zdiff3 {
            args.push("--zdiff3");
        }
        repo.tool_code(&args, 1);
        let expected = common::text(name, if zdiff3 { "output-zdiff3" } else { "output" })
            .replace("feature/ours", &ours)
            .replace("base-commit", &base)
            .replace("feature/theirs", "feature");
        assert_eq!(repo.content(&case.path), expected);
        for (stage, text) in case.texts().into_iter().enumerate() {
            assert_eq!(
                repo.blob(&format!(":{}:{}", stage + 1, case.path)),
                text.as_bytes()
            );
        }
    }
}

#[test]
fn merge_creates_strict_stages_when_native_git_would_clean() {
    let repo = Repo::new(BASE);
    repo.git(&["config", "core.autocrlf", "false"]);
    repo.diverge(OURS, THEIRS);
    let out = repo.tool(&["merge", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("ENTITY_CONFLICT"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("分析计划："));
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    let content = fs::read_to_string(repo.path().join("calc.go")).unwrap();
    assert!(
        content.contains("<<<<<<<") && content.contains("|||||||") && content.contains(">>>>>>>")
    );
    assert!(repo.path().join(".git/MERGE_HEAD").exists());
    assert_eq!(repo.blob(":1:calc.go"), BASE.as_bytes());
    assert_eq!(repo.blob(":2:calc.go"), OURS.as_bytes());
    assert_eq!(repo.blob(":3:calc.go"), THEIRS.as_bytes());
    assert!(content.contains("ENTITY_CONFLICT") && content.contains("<<<<<<<"));
    repo.git(&["merge", "--abort"]);
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        OURS
    );
}

#[test]
fn merge_clean_path_commits_normally() {
    let repo = Repo::new("package p\n\nfunc A() int { return 1 }\n");
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("new.go", "package p\n\nfunc B() int { return 2 }\n");
    repo.commit("theirs");
    repo.git(&["checkout", "-q", "main"]);
    let out = repo.tool(&["merge", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(repo.path().join("new.go").exists());
}

#[test]
fn cherry_pick_single_commit_uses_same_three_way_engine() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("calc.go", THEIRS);
    repo.commit("change");
    let commit = repo.git(&["rev-parse", "HEAD"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", OURS);
    repo.commit("ours");
    let out = repo.tool(&["cherry-pick", &commit]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    assert!(repo.path().join(".git/CHERRY_PICK_HEAD").exists());
    repo.git(&["cherry-pick", "--abort"]);
    assert_eq!(
        fs::read_to_string(repo.path().join("calc.go")).unwrap(),
        OURS
    );
}
