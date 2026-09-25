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
    assert!(String::from_utf8_lossy(&out.stderr).contains("分析结果："));
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
