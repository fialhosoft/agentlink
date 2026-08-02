//! The filesystem port.
//!
//! The domain never touches `std::fs`. It talks to this trait, which an adapter
//! implements against a real rooted directory (`agentlink-fs`) or against an
//! in-memory map (tests). Every path crossing the boundary is a [`RelPath`], so
//! the adapter owns all of the native-path, separator and privilege mess, and
//! the planner stays a pure function of observable state.

use crate::model::{Entry, LinkSupport, NodeKind, Via};
use crate::path::RelPath;

pub type FsResult<T> = Result<T, FsError>;

/// A filesystem operation that failed, with enough context to be actionable.
#[derive(Debug, thiserror::Error)]
#[error("failed to {operation} `{path}`")]
pub struct FsError {
    pub operation: &'static str,
    pub path: String,
    #[source]
    pub source: std::io::Error,
}

impl FsError {
    pub fn new(operation: &'static str, path: &RelPath, source: std::io::Error) -> Self {
        Self {
            operation,
            path: path.to_string(),
            source,
        }
    }
}

/// A rooted view of a workspace directory.
pub trait Workspace {
    /// Inspects an entry *without* following a trailing link.
    ///
    /// Returns `None` when nothing exists at `path`. A dangling link still
    /// returns `Some`, because for our purposes the link itself is the entry
    /// that occupies the path.
    fn probe(&self, path: &RelPath) -> FsResult<Option<Entry>>;

    /// Reads a UTF-8 file, following links.
    fn read(&self, path: &RelPath) -> FsResult<String>;

    /// Writes a UTF-8 file, creating parent directories as needed.
    fn write(&self, path: &RelPath, contents: &str) -> FsResult<()>;

    /// Creates a directory and all missing parents.
    fn create_dir_all(&self, path: &RelPath) -> FsResult<()>;

    /// Creates a link at `target` resolving to `canonical`, creating `target`'s
    /// parent directories as needed.
    ///
    /// The adapter decides how `canonical` is encoded: symlinks store a
    /// workspace-relative path so the workspace stays movable, while Windows
    /// junctions require an absolute one.
    fn link(&self, via: Via, node: NodeKind, canonical: &RelPath, target: &RelPath)
    -> FsResult<()>;

    /// Removes a link, never recursing into its contents.
    ///
    /// Implementations must refuse to delete a concrete directory: destroying
    /// user content is the one failure mode this tool cannot afford.
    fn remove_link(&self, path: &RelPath, node: NodeKind) -> FsResult<()>;

    /// Removes a regular file.
    fn remove_file(&self, path: &RelPath) -> FsResult<()>;

    /// Removes a directory, failing if it is not empty.
    ///
    /// Deliberately never recursive. Adoption needs to clear an empty canonical
    /// directory before moving content onto it; nothing in agentlink has a reason
    /// to delete a populated one.
    fn remove_empty_dir(&self, path: &RelPath) -> FsResult<()>;

    /// Moves an entry, creating the destination's parents as needed.
    fn rename(&self, from: &RelPath, to: &RelPath) -> FsResult<()>;

    /// Whether `path` is a directory containing no entries.
    fn is_empty_dir(&self, path: &RelPath) -> FsResult<bool>;

    /// Which link primitives this host currently permits.
    fn support(&self) -> LinkSupport;
}
