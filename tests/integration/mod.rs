use std::{
    fs,
    path::{Path, PathBuf},
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
        let full = self.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
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
    fn tool_code(&self, args: &[&str], code: i32) -> Output {
        let output = self.tool(args);
        assert_eq!(
            output.status.code(),
            Some(code),
            "strict-weave {args:?}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn content(&self, path: &str) -> String {
        fs::read_to_string(self.path().join(path)).unwrap()
    }
    fn blob(&self, spec: &str) -> Vec<u8> {
        let output = self.command("git", &["show", spec]);
        assert!(
            output.status.success(),
            "git show {spec}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    fn tree(&self, revision: &str) -> String {
        self.git(&["rev-parse", &format!("{revision}^{{tree}}")])
    }
    fn plans(&self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let directory = self.path().join(".git/strict-weave");
        let mut files = std::collections::BTreeMap::new();
        if directory.exists() {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path().join("plan.json");
                if path.is_file() {
                    files.insert(path.clone(), fs::read(path).unwrap());
                }
            }
        }
        files
    }
    fn copy_fixture_tree(&self, path: &str) {
        fn copy(source: &Path, target: &Path) {
            fs::create_dir_all(target).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let destination = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &destination);
                } else {
                    fs::copy(entry.path(), destination).unwrap();
                }
            }
        }
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(path),
            self.path(),
        );
    }
    fn rebase_state(&self) -> serde_json::Value {
        serde_json::from_str(&self.content(".git/strict-weave/rebase-state.json")).unwrap()
    }
}

const BASE: &str = include_str!("../fixtures/entities/disjoint.go.base");
const OURS: &str = include_str!("../fixtures/entities/disjoint.go.ours");
const THEIRS: &str = include_str!("../fixtures/entities/disjoint.go.theirs");

mod merge;
mod plans;
mod rebase;
mod rebase_global;
mod rebase_recovery;
mod stale_reports;
mod stash;
