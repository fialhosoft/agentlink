//! Workspace-relative paths.
//!
//! Every path the domain layer touches is expressed as a [`RelPath`]: a
//! normalised, always `/`-separated path rooted at the workspace. Adapters are
//! responsible for turning these into native paths.
//!
//! This buys us three things that matter a lot for a tool whose entire job is
//! creating links on three operating systems:
//!
//! 1. **No separator bugs.** `.agents/skills` and `.agents\skills` compare equal
//!    because only one spelling can ever exist.
//! 2. **No path traversal.** `..`, absolute paths, drive letters and UNC prefixes
//!    are rejected at construction. A provider manifest — which is community
//!    contributed data — therefore cannot address anything outside the workspace.
//! 3. **Trivially testable planning.** The domain never needs a real filesystem
//!    or a temp directory to reason about paths.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Reasons a string cannot be a workspace-relative path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("path must not be empty")]
    Empty,
    #[error("path must be relative to the workspace root, got `{0}`")]
    NotRelative(String),
    #[error("path must not escape the workspace with `..`, got `{0}`")]
    Escapes(String),
}

/// A normalised, `/`-separated path relative to the workspace root.
///
/// Guaranteed to be non-empty, to contain no `.`, `..` or empty segments, and to
/// be relative.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelPath(String);

impl RelPath {
    /// Parses and normalises a workspace-relative path.
    ///
    /// Accepts either separator, collapses redundant ones, and drops `.`
    /// segments. Rejects anything that could point outside the workspace.
    pub fn new(raw: &str) -> Result<Self, PathError> {
        let unified = raw.replace('\\', "/");
        let trimmed = unified.trim();
        if trimmed.is_empty() {
            return Err(PathError::Empty);
        }
        if trimmed.starts_with('/') {
            return Err(PathError::NotRelative(raw.to_string()));
        }
        // Windows drive-qualified (`C:\...`, `C:foo`) and UNC (`\\host\share`)
        // paths. The UNC case is already covered by the leading-slash check
        // after separator unification, but we keep the drive check explicit.
        let mut chars = trimmed.chars();
        if let (Some(first), Some(second)) = (chars.next(), chars.next())
            && first.is_ascii_alphabetic()
            && second == ':'
        {
            return Err(PathError::NotRelative(raw.to_string()));
        }

        let mut segments = Vec::new();
        for segment in trimmed.split('/') {
            match segment {
                "" | "." => {}
                ".." => return Err(PathError::Escapes(raw.to_string())),
                other => segments.push(other),
            }
        }
        if segments.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Self(segments.join("/")))
    }

    /// The normalised `/`-separated representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path segments, never empty.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The containing directory, or `None` when this path sits at the workspace
    /// root.
    pub fn parent(&self) -> Option<RelPath> {
        self.0
            .rsplit_once('/')
            .map(|(head, _)| Self(head.to_string()))
    }

    /// The final segment.
    pub fn file_name(&self) -> &str {
        self.0
            .rsplit_once('/')
            .map_or(self.0.as_str(), |(_, tail)| tail)
    }

    /// Appends a segment, normalising it in the process.
    pub fn join(&self, segment: &str) -> Result<Self, PathError> {
        Self::new(&format!("{}/{}", self.0, segment))
    }

    /// Renders `self` as a path relative to `dir`, where `None` means the
    /// workspace root.
    ///
    /// This is what a portable symlink stores: linking `.claude/skills` to
    /// `.agents/skills` should record `../.agents/skills`, not an absolute path,
    /// so the workspace stays movable.
    pub fn relative_to_dir(&self, dir: Option<&RelPath>) -> String {
        let from: Vec<&str> = dir.map(|d| d.segments().collect()).unwrap_or_default();
        let to: Vec<&str> = self.segments().collect();

        let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
        let mut parts: Vec<&str> = vec![".."; from.len() - shared];
        parts.extend_from_slice(&to[shared..]);

        if parts.is_empty() {
            ".".to_string()
        } else {
            parts.join("/")
        }
    }

    /// Whether `self` is `other` or lives underneath it.
    pub fn starts_with(&self, other: &RelPath) -> bool {
        self.0 == other.0
            || (self.0.starts_with(other.as_str())
                && self.0.as_bytes().get(other.as_str().len()) == Some(&b'/'))
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.0)
    }
}

impl TryFrom<String> for RelPath {
    type Error = PathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<RelPath> for String {
    fn from(value: RelPath) -> Self {
        value.0
    }
}

impl std::str::FromStr for RelPath {
    type Err = PathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(s: &str) -> RelPath {
        RelPath::new(s).expect("valid path")
    }

    #[test]
    fn normalises_separators_and_redundant_segments() {
        assert_eq!(rel(".claude\\skills").as_str(), ".claude/skills");
        assert_eq!(rel("./.agents//skills/").as_str(), ".agents/skills");
        assert_eq!(rel("AGENTS.md").as_str(), "AGENTS.md");
    }

    #[test]
    fn rejects_paths_that_could_escape_the_workspace() {
        assert_eq!(RelPath::new(""), Err(PathError::Empty));
        assert_eq!(RelPath::new("   "), Err(PathError::Empty));
        assert!(matches!(
            RelPath::new("/etc/passwd"),
            Err(PathError::NotRelative(_))
        ));
        assert!(matches!(
            RelPath::new("C:\\Windows"),
            Err(PathError::NotRelative(_))
        ));
        assert!(matches!(
            RelPath::new("\\\\host\\share"),
            Err(PathError::NotRelative(_))
        ));
        assert!(matches!(
            RelPath::new("../../.ssh/id_rsa"),
            Err(PathError::Escapes(_))
        ));
        assert!(matches!(
            RelPath::new(".claude/../../etc"),
            Err(PathError::Escapes(_))
        ));
    }

    #[test]
    fn parent_and_file_name() {
        assert_eq!(rel(".claude/skills").parent(), Some(rel(".claude")));
        assert_eq!(rel("CLAUDE.md").parent(), None);
        assert_eq!(rel(".claude/skills").file_name(), "skills");
        assert_eq!(rel("CLAUDE.md").file_name(), "CLAUDE.md");
    }

    #[test]
    fn relative_to_dir_produces_portable_link_targets() {
        // `.claude/skills` -> `.agents/skills` must be stored as `../.agents/skills`.
        assert_eq!(
            rel(".agents/skills").relative_to_dir(Some(&rel(".claude"))),
            "../.agents/skills"
        );
        // A root-level link records a bare name.
        assert_eq!(rel("AGENTS.md").relative_to_dir(None), "AGENTS.md");
        // Deeper nesting walks back the right number of levels.
        assert_eq!(
            rel(".agents/skills").relative_to_dir(Some(&rel("a/b/c"))),
            "../../../.agents/skills"
        );
        // Shared prefixes are not walked back over.
        assert_eq!(
            rel(".agents/skills").relative_to_dir(Some(&rel(".agents"))),
            "skills"
        );
    }

    #[test]
    fn starts_with_respects_segment_boundaries() {
        assert!(rel(".agents/skills").starts_with(&rel(".agents")));
        assert!(rel(".agents").starts_with(&rel(".agents")));
        // `.agentsfoo` is not inside `.agents`.
        assert!(!rel(".agentsfoo/x").starts_with(&rel(".agents")));
    }
}
