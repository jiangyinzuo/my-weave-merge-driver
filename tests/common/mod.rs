//! Shared fixture access. Read exact bytes; never synthesize merge source text.
#![allow(dead_code)]
use serde::Deserialize;
use std::{fs, path::PathBuf};
use strict_weave::merge::{self, Labels, Outcome};

pub fn fixture_path(name: &str, suffix: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.{suffix}"))
}

pub fn text(name: &str, suffix: &str) -> String {
    let path = fixture_path(name, suffix);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub const STALE_TEXT: &str = "stale ours";

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub marker_size: usize,
    pub ours_label: String,
    pub base_label: String,
    pub theirs_label: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            marker_size: 7,
            ours_label: "feature/ours".into(),
            base_label: "base-commit".into(),
            theirs_label: "feature/theirs".into(),
        }
    }
}
impl Options {
    pub fn load(name: &str) -> Result<Self, String> {
        let path = fixture_path(name, "options");
        if path.exists() {
            let contents =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::from_str(&contents).map_err(|e| format!("{}: {e}", path.display()))
        } else {
            Ok(Self::default())
        }
    }
    pub fn labels(&self) -> Labels {
        Labels {
            ours: self.ours_label.clone(),
            base: self.base_label.clone(),
            theirs: self.theirs_label.clone(),
        }
    }
}

pub struct Case {
    pub name: String,
    pub path: String,
    pub base: String,
    pub ours: String,
    pub theirs: String,
}
impl Case {
    pub fn load(name: &str) -> Self {
        let path = if fixture_path(name, "path").exists() {
            text(name, "path").trim_end_matches(['\r', '\n']).to_owned()
        } else {
            name.to_owned()
        };
        Self {
            name: name.into(),
            path,
            base: text(name, "base"),
            ours: text(name, "ours"),
            theirs: text(name, "theirs"),
        }
    }
    pub fn texts(&self) -> [&str; 3] {
        [&self.base, &self.ours, &self.theirs]
    }
    pub fn run(&self) -> Outcome {
        let options = Options::load(&self.name).unwrap();
        merge::merge(
            &self.base,
            &self.ours,
            &self.theirs,
            &self.path,
            &options.labels(),
            options.marker_size,
        )
        .unwrap()
    }
    pub fn expected(&self) -> String {
        text(&self.name, "output")
    }
}
