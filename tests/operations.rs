use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Repo(TempDir);
impl Repo {
    fn new(base: &str) -> Self {
        let repo = Self(tempfile::tempdir().unwrap());
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo.git(&["config", "commit.gpgSign", "false"]);
        repo.write("calc.go", base);
        repo.commit("base");
        repo
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn command(&self, program: &str, args: &[&str]) -> Output {
        Command::new(program)
            .current_dir(self.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("LC_ALL", "C")
            .output()
            .unwrap_or_else(|e| panic!("run {program} {args:?}: {e}"))
    }
    fn git(&self, args: &[&str]) -> String {
        let out = self.command("git", args);
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.path().join(path), text).unwrap();
    }
    fn commit(&self, msg: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", msg]);
    }
    fn diverge(&self, ours: &str, theirs: &str) {
        self.git(&["checkout", "-qb", "feature"]);
        self.write("calc.go", theirs);
        self.commit("theirs");
        self.git(&["checkout", "-q", "main"]);
        self.write("calc.go", ours);
        self.commit("ours");
    }
    fn tool(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_strict-weave"), args)
    }
}

const BASE: &str = include_str!("fixtures/entities/disjoint.go.base");
const OURS: &str = include_str!("fixtures/entities/disjoint.go.ours");
const THEIRS: &str = include_str!("fixtures/entities/disjoint.go.theirs");

#[test]
fn merge_analyzes_all_trees_and_leaves_standard_conflict_state() {
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
}

#[test]
fn merge_creates_strict_stages_when_native_git_would_clean() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let out = repo.tool(&["merge", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(repo.git(&["ls-files", "-u"]).contains("calc.go"));
    let content = fs::read_to_string(repo.path().join("calc.go")).unwrap();
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

#[test]
fn plan_only_is_read_only_and_apply_consumes_the_bound_plan() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("merge-plan.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["merge", "feature", "--plan", "--output", plan_text]);
    assert_eq!(planned.status.code(), Some(1));
    assert!(plan.is_file());
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/MERGE_HEAD").exists());

    let applied = repo.tool(&["merge", "feature", "--apply", plan_text]);
    assert_eq!(
        applied.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
}

#[test]
fn stale_plan_is_rejected_before_git_state_changes() {
    let repo = Repo::new(BASE);
    repo.diverge(OURS, THEIRS);
    let output = tempfile::tempdir().unwrap();
    let plan = output.path().join("stale.json");
    let plan_text = plan.to_str().unwrap();
    let planned = repo.tool(&["merge", "feature", "--plan", "--output", plan_text]);
    assert_eq!(planned.status.code(), Some(1));
    repo.write("unrelated.txt", "changed after planning\n");
    repo.commit("advance ours");
    let before = repo.git(&["rev-parse", "HEAD"]);
    let applied = repo.tool(&["merge", "feature", "--apply", plan_text]);
    assert_eq!(applied.status.code(), Some(129));
    assert!(String::from_utf8_lossy(&applied.stderr).contains("分析计划"));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before);
    assert!(repo.git(&["ls-files", "-u"]).is_empty());
}

#[test]
fn rebase_single_commit_leaves_native_rebase_state_for_continue() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("calc.go", THEIRS);
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("calc.go", OURS);
    repo.commit("ours");
    let out = repo.tool(&["rebase", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(repo.path().join(".git/rebase-merge").exists());
    assert!(!repo.git(&["ls-files", "-u"]).is_empty());
    repo.write("calc.go", "resolved by rebase\n");
    repo.git(&["add", "calc.go"]);
    let continued = repo.command("git", &["rebase", "--continue"]);
    assert!(
        continued.status.success(),
        "{}",
        String::from_utf8_lossy(&continued.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/rebase-merge").exists());
}

#[test]
fn clean_single_commit_rebase_updates_branch_without_rebase_state() {
    let repo = Repo::new(BASE);
    repo.git(&["checkout", "-qb", "feature"]);
    repo.write("feature.go", "package p\n");
    repo.commit("feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main.go", "package p\n");
    repo.commit("main");
    let out = repo.tool(&["rebase", "feature"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
    assert!(!repo.path().join(".git/rebase-merge").exists());
    assert_eq!(repo.git(&["rev-list", "--count", "feature..HEAD"]), "1");
    assert!(repo.path().join("main.go").exists());
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
