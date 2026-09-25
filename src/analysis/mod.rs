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
    analyze, augment, blob_oid, load_for_driver, prepare, save, snapshot, FileAnalysis, Location,
    MoveCandidate, Report,
};
