//! Full-tree analysis followed by explicit Git operations.
//!
//! `plan` owns snapshot validation and conflict installation; `git` owns shared
//! plumbing. Each operation owns its execution and recovery decisions.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod git;
mod merge;
mod plan;
mod rebase;
mod stash;
pub use merge::{cherry_pick, merge};
pub use rebase::{rebase, rebase_abort, rebase_continue};
pub use stash::stash;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub zdiff3: bool,
    pub detailed: bool,
}

pub enum Mode {
    Apply,
    Plan(PathBuf),
    ApplyPlan(PathBuf),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OperationKind {
    Merge,
    CherryPick,
    Rebase,
    Stash,
}

impl OperationKind {
    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::CherryPick => "cherry-pick",
            Self::Rebase => "rebase",
            Self::Stash => "stash",
        }
    }
}
