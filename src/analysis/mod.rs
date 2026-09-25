#![doc = include_str!("../../docs/analysis.md")]

mod global;
mod line;
pub(crate) mod local;
mod moves;
mod partition;
mod raw;
mod syntax;
mod upstream;

// Existing library API for explicit three-tree reports.
pub use global::{
    analyze, apply_file_analysis, blob_oid, load_file_analysis, save, snapshot, FileAnalysis,
    Location, MoveCandidate, Report, TreeSnapshot,
};
