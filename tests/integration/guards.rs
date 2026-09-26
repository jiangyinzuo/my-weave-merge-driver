use super::*;

fn rejected_merge(repo: &Repo, message: &str) {
    let head = repo.git(&["rev-parse", "HEAD"]);
    let refs = repo.git(&["show-ref"]);
    let status = repo.git(&["status", "--porcelain"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let content = repo.content("calc.go");
    let output = repo.tool_code(&["merge", "feature"], 129);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["show-ref"]), refs);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(repo.git(&["status", "--porcelain"]), status);
    assert_eq!(repo.content("calc.go"), content);
    assert!(!repo.path().join(".git/MERGE_HEAD").exists());
    assert!(!repo
        .path()
        .join(".git/strict-weave/operation.lock")
        .exists());
}

#[test]
fn dirty_staged_and_untracked_changes_are_rejected_without_mutation() {
    for kind in ["worktree", "index", "untracked"] {
        let repo = Repo::new(BASE);
        repo.diverge(OURS, THEIRS);
        if kind == "untracked" {
            repo.write("local.go", BASE);
        } else {
            repo.write("calc.go", BASE);
            if kind == "index" {
                repo.git(&["add", "calc.go"]);
            }
        }
        rejected_merge(&repo, "工作区或 index 不干净");
        if kind == "untracked" {
            assert_eq!(repo.content("local.go"), BASE);
        }
    }
}

#[test]
fn attributes_and_checkout_conversions_are_rejected_before_applying() {
    for attribute in [
        "merge=ours",
        "-merge",
        "filter=custom",
        "working-tree-encoding=UTF-16",
        "ident",
        "text",
        "eol=lf",
    ] {
        let repo = Repo::new(BASE);
        repo.diverge(OURS, THEIRS);
        repo.write(".git/info/attributes", &format!("calc.go {attribute}\n"));
        rejected_merge(&repo, "attribute");
    }
    for (key, value) in [("core.autocrlf", "input"), ("core.sparseCheckout", "true")] {
        let repo = Repo::new(BASE);
        repo.diverge(OURS, THEIRS);
        repo.git(&["config", key, value]);
        rejected_merge(
            &repo,
            if key == "core.autocrlf" {
                key
            } else {
                "sparse checkout"
            },
        );
    }
    // A target branch's attributes must also be checked before checkout.
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.git(&["checkout", "-q", "feature"]);
    repo.write(".gitattributes", "calc.go merge=ours\n");
    repo.commit("target attributes");
    repo.git(&["checkout", "-q", "main"]);
    rejected_merge(&repo, "attribute");
}

#[test]
fn unsupported_tree_shapes_and_binary_inputs_are_rejected_before_applying() {
    for kind in ["rename", "directory", "binary"] {
        let repo = Repo::new(BASE);
        repo.git(&["checkout", "-qb", "feature"]);
        match kind {
            "rename" => {
                repo.git(&["mv", "calc.go", "renamed.go"]);
            }
            "directory" => repo.write("entry/child.go", THEIRS),
            "binary" => {
                fs::copy(
                    common::fixture_path("fallback/binary-input.go", "ours"),
                    repo.path().join("binary.go"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        repo.commit("unsupported target");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("calc.go", OURS);
        if kind == "directory" {
            repo.write("entry", BASE);
        }
        repo.commit("ours");
        rejected_merge(
            &repo,
            match kind {
                "rename" => "Git 文件 rename",
                "directory" => "文件/目录布局冲突",
                "binary" => "binary",
                _ => unreachable!(),
            },
        );
        assert!(!repo.path().join("binary.go").exists());
        assert!(!repo.path().join("renamed.go").exists());
        if kind == "directory" {
            assert_eq!(repo.content("entry"), BASE);
        }
    }
}

#[cfg(unix)]
#[test]
fn symlinks_in_target_are_rejected_before_checkout() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    repo.git(&["checkout", "-q", "feature"]);
    std::os::unix::fs::symlink("calc.go", repo.path().join("linked.go")).unwrap();
    repo.commit("symlink");
    repo.git(&["checkout", "-q", "main"]);
    rejected_merge(&repo, "symlink/submodule");
    assert!(!repo.path().join("linked.go").exists());
}
