#![doc = include_str!("../../docs/analysis.md")]

mod git;
mod global;
mod line;
pub(crate) mod local;
mod moves;
mod partition;
mod raw;
mod syntax;
mod upstream;

// Existing library API for explicit three-tree reports.
pub(crate) use git::GitSnapshot;
pub use git::{blob_oid, snapshot};
pub use global::{
    analyze, apply_file_analysis, load_file_analysis, save, FileAnalysis, Location, MoveCandidate,
    Report, TreeSnapshot,
};
