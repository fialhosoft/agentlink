//! The agentlink domain.
//!
//! agentlink gives every AI coding agent in a repository the same brain, without
//! copying a single file. It does that by exploiting a fact about the 2026
//! ecosystem: for the resources that matter most, **the format has already
//! converged and only the location differs.**
//!
//! * Instructions converged on `AGENTS.md` (Linux Foundation, December 2025).
//! * Skills converged on Agent Skills / `SKILL.md` (open specification,
//!   December 2025), read by 30+ tools.
//!
//! So each pairing of an agent with a resource resolves to one of four verdicts:
//!
//! | Verdict                                       | Meaning                                   | Cost |
//! |-----------------------------------------------|-------------------------------------------|------|
//! | [`Native`](model::Strategy::Native)            | the tool already reads the canonical path | zero |
//! | [`Link`](model::Strategy::Link)                | same format, different path               | one link |
//! | [`Import`](model::Strategy::Import)            | link impossible; write a one-line stub    | one small file |
//! | [`plan::Blocked`]                              | needs a human decision                    | nothing is written |
//!
//! The payoff is that bidirectional propagation is not a feature anyone has to
//! implement. For a `Native` or `Link` verdict there is exactly one inode, so
//! editing, renaming, moving or deleting through any agent's path is immediately
//! visible to every other agent — with no daemon, no watcher and no
//! reconciliation pass.
//!
//! # Layering
//!
//! ```text
//! agentlink-cli    composition root, human-facing output
//!       ↓
//! agentlink-domain   this crate: pure domain, no `std::fs`
//!       ↑
//! agentlink-fs     adapter implementing [`Workspace`] against a real directory
//! ```
//!
//! Everything here is deterministic and testable without a filesystem: planning
//! is a pure function of observable state, and [`testing::FakeWorkspace`]
//! simulates hosts — including Windows without symlink privileges — that a given
//! CI runner cannot otherwise reproduce.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, clippy::doc_markdown)]

pub mod apply;
pub mod config;
pub mod detect;
pub mod gitignore;
pub mod layout;
pub mod lock;
pub mod model;
pub mod path;
pub mod plan;
pub mod provider;
pub mod registry;
pub mod workspace;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use apply::{ApplyReport, apply};
pub use config::Config;
pub use layout::Layout;
pub use lock::Lock;
pub use model::{LinkSupport, LinkTarget, NodeKind, ResourceKind, Strategy, Via};
pub use path::RelPath;
pub use plan::{Blocked, Outcome, Plan, Planner, Skip, Step};
pub use provider::{Capability, Provider};
pub use registry::Registry;
pub use workspace::{FsError, FsResult, Workspace};

/// The version of the `agentlink` crate, surfaced in `--version` and lock files.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
