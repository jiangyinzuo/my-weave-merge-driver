mod common;
use common::{text, Case, STALE_TEXT};
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const ATTRIBUTES: &str = r#"*.go merge=strict-weave
"#;
const STAGED_TEXT: &str = "staged";
const DIRTY_TEXT: &str = "dirty worktree";
const UNTRACKED_TEXT: &str = "untracked";

const BASE: &str = include_str!("fixtures/entities/disjoint.go.base");

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
    std::fs::write(root.join(".gitattributes"), ATTRIBUTES).unwrap();
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
        &text("entities/disjoint.go", "ours"),
        &text("entities/disjoint.go", "theirs"),
    );
    let out = command("git", root, &["merge", "--no-edit", "other"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("ENTITY_CONFLICT"));
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
    let c = Case::load("fallback/binary-input.go");
    for (path, contents) in ["base", "ours", "theirs"].into_iter().zip(c.texts()) {
        std::fs::write(root.join(path), contents).unwrap();
    }
    let out = tool(root, &["driver", "base", "ours", "theirs", "a.go"]);
    assert_eq!(out.status.code(), Some(129));
    assert_eq!(std::fs::read(root.join("ours")).unwrap(), c.ours.as_bytes());
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
    std::fs::write(root.join(".git/info/attributes"), ATTRIBUTES).unwrap();
}

#[test]
fn prepare_reads_explicit_trees_without_changing_dirty_repository() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    let changed = text("entities/unilateral.go", "ours");
    std::fs::write(root.join("calc.go"), &changed).unwrap();
    git(root, &["commit", "-qam", "change"]);
    std::fs::write(root.join("staged"), STAGED_TEXT).unwrap();
    git(root, &["add", "staged"]);
    std::fs::write(root.join("calc.go"), DIRTY_TEXT).unwrap();
    std::fs::write(root.join("untracked"), UNTRACKED_TEXT).unwrap();
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
        DIRTY_TEXT.as_bytes()
    );
    assert_eq!(
        std::fs::read(root.join("staged")).unwrap(),
        STAGED_TEXT.as_bytes()
    );
    assert_eq!(
        std::fs::read(root.join("untracked")).unwrap(),
        UNTRACKED_TEXT.as_bytes()
    );
    assert_eq!(tool(root, &args).status.code(), Some(129));
    assert_eq!(bytes, std::fs::read(&file).unwrap());
}

#[test]
fn real_driver_reads_move_context_and_git_owns_continue_abort() {
    let c = Case::load("moves/move-source.go");
    let temp = init("source.go", &c.base);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    git(root, &["checkout", "-qb", "other"]);
    std::fs::write(root.join("source.go"), &c.theirs).unwrap();
    std::fs::write(
        root.join("target.go"),
        text("moves/move-target.go", "theirs"),
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "move"]);
    git(root, &["checkout", "-q", "main"]);
    let ours = &c.ours;
    std::fs::write(root.join("source.go"), ours).unwrap();
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
        *ours
    );
    assert!(git(root, &["ls-files", "-u"]).is_empty());
}

#[test]
fn preparation_detects_identical_edits_even_when_native_git_skips_driver() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    let changed = text("entities/unilateral.go", "ours");
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
    let changed = text("entities/unilateral.go", "ours");
    diverge(
        root,
        "calc.go",
        &changed,
        &text("entities/opposed.go", "theirs"),
    );
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
    for (name, text) in [("b", BASE), ("o", STALE_TEXT), ("t", BASE)] {
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
    assert_eq!(
        std::fs::read(root.join("o")).unwrap(),
        STALE_TEXT.as_bytes()
    );
    for cmd in ["diff", "merge", "rebase", "cherry-pick", "stash"] {
        assert_eq!(tool(root, &[cmd]).status.code(), Some(2));
    }
}

#[test]
fn analysis_environment_and_explicit_override_work_and_input_errors_write_no_report() {
    let temp = init("calc.go", BASE);
    let root = temp.path();
    let ours = text("entities/unilateral.go", "ours");
    let theirs = text("entities/opposed.go", "theirs");
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
    std::fs::write(
        root.join("binary"),
        text("fallback/binary-input.go", "ours"),
    )
    .unwrap();
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
