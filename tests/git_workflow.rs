use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const BASE: &str = "func a() int {\n    x := 1\n    y := 2\n    z := 3\n    return y\n}\n";

fn command(program: &str, root: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .env("GIT_EDITOR", "true")
        .env_remove("STRICT_WEAVE_ANALYSIS")
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = command("git", root, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn tool(root: &Path, args: &[&str]) -> Output {
    command(env!("CARGO_BIN_EXE_strict-weave"), root, args)
}

fn init(path: &str, base: &str) -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "-q", "-b", "main"]);
    git(temp.path(), &["config", "user.name", "Test"]);
    git(
        temp.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    git(temp.path(), &["config", "commit.gpgSign", "false"]);
    git(temp.path(), &["config", "core.autocrlf", "false"]);
    std::fs::write(temp.path().join(path), base).unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "base"]);
    temp
}

fn diverge(root: &Path, path: &str, ours: &str, theirs: &str) {
    git(root, &["checkout", "-qb", "other"]);
    std::fs::write(root.join(path), theirs).unwrap();
    git(root, &["commit", "-qam", "theirs"]);
    git(root, &["checkout", "-q", "main"]);
    std::fs::write(root.join(path), ours).unwrap();
    git(root, &["commit", "-qam", "ours"]);
}

#[test]
fn real_merge_driver_blocks_disjoint_function_edits() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    std::fs::write(root.join(".gitattributes"), "*.go merge=strict-weave\n").unwrap();
    git(root, &["add", ".gitattributes"]);
    git(root, &["commit", "-qm", "attributes"]);
    let executable = env!("CARGO_BIN_EXE_strict-weave").replace('\'', "'\\''");
    let driver = format!(
        "'{executable}' driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y"
    );
    git(root, &["config", "merge.strict-weave.driver", &driver]);
    diverge(
        root,
        "calc.go",
        &BASE.replace("    z := 3\n", ""),
        &BASE.replace("    x := 1\n", ""),
    );
    let out = command("git", root, &["merge", "--no-edit", "other"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("ENTITY_BOTH_CHANGED"));
    assert!(!git(root, &["ls-files", "-u"]).is_empty());
    let content = std::fs::read_to_string(root.join("calc.go")).unwrap();
    assert!(content.contains("<<<<<<< ours: HEAD"));
    assert!(content.contains(">>>>>>> theirs: other"));
    assert_eq!(content.matches("func a()").count(), 3);
}

#[test]
fn driver_errors_do_not_overwrite_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (path, contents) in [("base", "old"), ("ours", "ours\0data"), ("theirs", "new")] {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let out = tool(root, &["driver", "base", "ours", "theirs", "a.go"]);
    assert_eq!(out.status.code(), Some(129));
    assert_eq!(std::fs::read(root.join("ours")).unwrap(), b"ours\0data");
}

fn configure_driver(root: &Path, analysis: Option<&Path>) {
    let executable = env!("CARGO_BIN_EXE_strict-weave").replace('\'', "'\\''");
    let extra = analysis
        .map(|p| {
            format!(
                " --analysis '{}'",
                p.display().to_string().replace('\'', "'\\''")
            )
        })
        .unwrap_or_default();
    git(
        root,
        &[
            "config",
            "merge.strict-weave.driver",
            &format!("'{executable}' driver %O %A %B %P %L{extra}"),
        ],
    );
    std::fs::write(
        root.join(".git/info/attributes"),
        "*.go merge=strict-weave\n",
    )
    .unwrap();
}

#[test]
fn prepare_reads_explicit_trees_without_changing_dirty_repository() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    let changed = BASE.replace("y := 2", "y := 5");
    std::fs::write(root.join("calc.go"), &changed).unwrap();
    git(root, &["commit", "-qam", "change"]);
    std::fs::write(root.join("staged"), "staged").unwrap();
    git(root, &["add", "staged"]);
    std::fs::write(root.join("calc.go"), "dirty worktree").unwrap();
    std::fs::write(root.join("untracked"), "untracked").unwrap();
    // These must never be invoked by object-only preparation.
    git(root, &["config", "diff.external", "false"]);
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let head = std::fs::read(root.join(".git/HEAD")).unwrap();
    let refs = git(root, &["show-ref"]);
    let destination = tempfile::tempdir().unwrap();
    let file = destination.path().join("analysis.json");
    let args = [
        "driver",
        "prepare",
        &base,
        "HEAD",
        "HEAD",
        "--output",
        file.to_str().unwrap(),
    ];
    let output = tool(root, &args);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let bytes = std::fs::read(&file).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        report["trees"][0],
        git(root, &["rev-parse", &format!("{base}^{{tree}}")]).trim()
    );
    assert_eq!(index, std::fs::read(root.join(".git/index")).unwrap());
    assert_eq!(head, std::fs::read(root.join(".git/HEAD")).unwrap());
    assert_eq!(refs, git(root, &["show-ref"]));
    assert_eq!(
        std::fs::read(root.join("calc.go")).unwrap(),
        b"dirty worktree"
    );
    assert_eq!(std::fs::read(root.join("staged")).unwrap(), b"staged");
    assert_eq!(std::fs::read(root.join("untracked")).unwrap(), b"untracked");
    assert_eq!(tool(root, &args).status.code(), Some(129));
    assert_eq!(bytes, std::fs::read(&file).unwrap());
}

#[test]
fn real_driver_reads_move_context_and_git_owns_continue_abort() {
    let keep = "\nfunc retained() int {\n    return 10\n}\n";
    let text = format!("{BASE}{keep}");
    let temp = init("source.go", &text);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    git(root, &["checkout", "-qb", "other"]);
    std::fs::write(root.join("source.go"), keep).unwrap();
    std::fs::write(root.join("target.go"), BASE).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "move"]);
    git(root, &["checkout", "-q", "main"]);
    let ours = text.replace("y := 2", "y := 5");
    std::fs::write(root.join("source.go"), &ours).unwrap();
    git(root, &["commit", "-qam", "edit"]);
    let destination = tempfile::tempdir().unwrap();
    let file = destination.path().join("analysis.json");
    let output = tool(
        root,
        &[
            "driver",
            "prepare",
            &base,
            "HEAD",
            "other",
            "--output",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = std::fs::read(&file).unwrap();
    configure_driver(root, Some(&file));
    let output = command(
        "git",
        root,
        &["-c", "merge.renames=false", "merge", "--no-edit", "other"],
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MOVE_CANDIDATE") && stderr.contains("target.go"),
        "{stderr}"
    );
    assert!(!git(root, &["ls-files", "-u"]).is_empty());
    assert_eq!(report, std::fs::read(&file).unwrap());
    git(root, &["merge", "--abort"]);
    assert_eq!(
        std::fs::read_to_string(root.join("source.go")).unwrap(),
        ours
    );
    assert!(git(root, &["ls-files", "-u"]).is_empty());
}

#[test]
fn preparation_detects_identical_edits_even_when_native_git_skips_driver() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    let changed = BASE.replace("y := 2", "y := 5");
    diverge(root, "calc.go", &changed, &changed);
    let destination = tempfile::tempdir().unwrap();
    let file = destination.path().join("analysis.json");
    assert_eq!(
        tool(
            root,
            &[
                "driver",
                "prepare",
                &base,
                "HEAD",
                "other",
                "--output",
                file.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(1)
    );
    configure_driver(root, Some(&file));
    // This intentionally demonstrates a boundary, not the desired strict policy:
    // ignoring prepare's exit=1 lets Git skip the driver and accept equal blobs.
    git(root, &["merge", "--no-edit", "other"]);
    assert!(git(root, &["ls-files", "-u"]).is_empty());
    assert_eq!(
        std::fs::read_to_string(root.join("calc.go")).unwrap(),
        changed
    );
}

#[test]
fn stale_analysis_fails_before_driver_writes_and_no_workflow_or_diff_commands_exist() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let changed = BASE.replace("y := 2", "y := 5");
    diverge(root, "calc.go", &changed, &BASE.replace("y := 2", "y := 9"));
    let destination = tempfile::tempdir().unwrap();
    let file = destination.path().join("analysis.json");
    assert_eq!(
        tool(
            root,
            &[
                "driver",
                "prepare",
                "HEAD~1",
                "HEAD",
                "other",
                "--output",
                file.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(1)
    );
    for (name, text) in [("b", BASE), ("o", "stale ours"), ("t", BASE)] {
        std::fs::write(root.join(name), text).unwrap();
    }
    let output = tool(
        root,
        &[
            "driver",
            "b",
            "o",
            "t",
            "calc.go",
            "--analysis",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(129));
    assert!(String::from_utf8_lossy(&output.stderr).contains("输入不匹配"));
    assert_eq!(std::fs::read(root.join("o")).unwrap(), b"stale ours");
    for cmd in ["diff", "merge", "rebase", "cherry-pick", "stash"] {
        assert_eq!(tool(root, &[cmd]).status.code(), Some(2));
    }
}

#[test]
fn analysis_environment_and_explicit_override_work_and_input_errors_write_no_report() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let ours = BASE.replace("y := 2", "y := 5");
    let theirs = BASE.replace("y := 2", "y := 9");
    diverge(root, "calc.go", &ours, &theirs);
    let destination = tempfile::tempdir().unwrap();
    let file = destination.path().join("analysis.json");
    assert_eq!(
        tool(
            root,
            &[
                "driver",
                "prepare",
                "HEAD~1",
                "HEAD",
                "other",
                "--output",
                file.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(1)
    );
    let before = std::fs::read(&file).unwrap();
    for explicit in [false, true] {
        for (name, text) in [("b", BASE), ("o", &ours), ("t", &theirs)] {
            std::fs::write(root.join(name), text).unwrap();
        }
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_strict-weave"));
        cmd.current_dir(root)
            .args(["driver", "b", "o", "t", "calc.go"]);
        if explicit {
            cmd.env("STRICT_WEAVE_ANALYSIS", "/nonexistent-analysis.json")
                .arg("--analysis")
                .arg(&file);
        } else {
            cmd.env("STRICT_WEAVE_ANALYSIS", &file);
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(std::fs::read_to_string(root.join("o"))
            .unwrap()
            .contains("<<<<<<<"));
        assert_eq!(std::fs::read(&file).unwrap(), before);
    }
    let missing = destination.path().join("should-not-exist.json");
    assert_eq!(
        tool(
            root,
            &[
                "driver",
                "prepare",
                "no-such-revision",
                "HEAD",
                "other",
                "--output",
                missing.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(129)
    );
    assert!(!missing.exists());
    std::fs::write(root.join("binary"), b"binary\0data").unwrap();
    git(root, &["add", "binary"]);
    git(root, &["commit", "-qm", "binary"]);
    assert_eq!(
        tool(
            root,
            &[
                "driver",
                "prepare",
                "HEAD~1",
                "HEAD",
                "other",
                "--output",
                missing.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(129)
    );
    assert!(!missing.exists());
}
