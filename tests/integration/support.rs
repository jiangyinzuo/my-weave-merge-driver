use crate::common::{fixture_path, text};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

pub const TOOL: &str = env!("CARGO_BIN_EXE_strict-weave");
pub const SIMPLE: &str = "entities/disjoint.go";

pub struct Repo(pub TempDir);
impl Repo {
    pub fn new(case: &str, path: &str) -> Self {
        // Spaces and quotes also exercise report paths and shell quoting.
        let repo = Self(
            tempfile::Builder::new()
                .prefix("strict weave's integration-")
                .tempdir_in("/tmp")
                .unwrap(),
        );
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Integration Test"]);
        repo.git(&["config", "user.email", "integration@example.invalid"]);
        repo.git(&["config", "commit.gpgSign", "false"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo.git(&["config", "core.hooksPath", "/dev/null"]);
        repo.copy(case, "base", path);
        fs::write(
            repo.path().join(".gitattributes"),
            "*.go merge=strict-weave\n*.rs merge=strict-weave\n",
        )
        .unwrap();
        repo.commit("base");
        repo
    }
    pub fn path(&self) -> &Path {
        self.0.path()
    }
    pub fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(self.path()).args(args);
        // Isolate from the developer's Git configuration, current Git operation,
        // signing, editor and externally supplied analysis report.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .env("LC_ALL", "C")
            .env_remove("STRICT_WEAVE_ANALYSIS");
        cmd
    }
    pub fn git(&self, args: &[&str]) -> String {
        let out = self.command("git", args).output().unwrap();
        assert_code(&out, 0);
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .into()
    }
    pub fn copy(&self, case: &str, side: &str, path: &str) {
        let dest = self.path().join(path);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(fixture_path(case, side), dest).unwrap();
    }
    pub fn commit(&self, message: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", message]);
    }
    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
    pub fn diverge(&self, case: &str, path: &str) {
        self.git(&["checkout", "-qb", "other"]);
        self.copy(case, "theirs", path);
        self.commit("theirs");
        self.git(&["checkout", "-q", "main"]);
        self.copy(case, "ours", path);
        self.commit("ours");
    }
    pub fn assert_file(&self, path: &str, case: &str, side: &str) {
        assert_eq!(
            fs::read_to_string(self.path().join(path)).unwrap(),
            text(case, side)
        );
    }
    pub fn assert_clean(&self) {
        assert_eq!(self.git(&["status", "--porcelain"]), "");
    }
}
pub fn assert_code(out: &Output, expected: i32) {
    assert_eq!(
        out.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into()
}
