#![doc = include_str!("../../docs/analysis.md")]

mod global;
mod line;
pub(crate) mod local;
mod moves;
mod partition;
mod raw;
mod upstream;

// Existing library API for explicit three-tree reports.
pub use global::{
    analyze, augment, load_for_driver, prepare, save, FileAnalysis, Location, MoveCandidate, Report,
};
