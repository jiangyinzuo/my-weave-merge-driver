//! Real Git repositories, CLI processes and persisted state (no mocked Git).
mod common;
#[path = "integration/merge.rs"]
mod merge;
#[path = "integration/native.rs"]
mod native;
#[path = "integration/pull_stash.rs"]
mod pull_stash;
#[path = "integration/rebase.rs"]
mod rebase;
#[path = "integration/support.rs"]
mod support;
