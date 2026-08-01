//! Core vocabulary: what can be shared, and how it gets put in place.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::path::RelPath;

/// A class of configuration that agents can share.
///
/// Adding a variant is the main axis of growth for this project; see
/// `docs/adr/0004-resource-kinds.md` for the criteria a new resource must meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    /// Project-wide instructions. Canonically `AGENTS.md`.
    Instructions,
    /// Agent Skills directories, each holding a `SKILL.md`. Canonically
    /// `.agents/skills`.
    Skills,
}

impl ResourceKind {
    /// Every resource kind, in stable display order.
    pub const ALL: &'static [ResourceKind] = &[Self::Instructions, Self::Skills];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Instructions => "instructions",
            Self::Skills => "skills",
        }
    }

    /// Whether the canonical resource is a file or a directory.
    ///
    /// This decides which linking primitives are even applicable: directories can
    /// use junctions on Windows, files cannot.
    pub const fn node(self) -> NodeKind {
        match self {
            Self::Instructions => NodeKind::File,
            Self::Skills => NodeKind::Dir,
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ResourceKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| format!("unknown resource `{s}`"))
    }
}

/// Whether a filesystem entry is a file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    File,
    Dir,
}

/// How a provider obtains a resource.
///
/// Declared per capability in a provider manifest. The planner turns a strategy
/// into a concrete [`Via`] once it knows what the host filesystem supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    /// The tool already reads the canonical path. Nothing is written, ever.
    ///
    /// This is the best possible outcome and the reason the canonical layout was
    /// chosen to match the emerging standards rather than invent a new one.
    Native,
    /// Same format, different location: materialise a filesystem link so both
    /// paths resolve to one inode.
    Link,
    /// Same format, but write a small stub that references the canonical file
    /// using the tool's own include syntax. Used where linking is impossible.
    Import,
}

impl Strategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Link => "link",
            Self::Import => "import",
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The concrete mechanism used to materialise a capability.
///
/// Note the deliberate absence of hard links: they are indistinguishable from
/// regular files after the fact, and any editor performing an atomic save
/// (write-temp-then-rename) silently breaks the association, leaving two files
/// that diverge with no way to detect it. See `docs/adr/0003-link-primitives.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    /// A symbolic link storing a workspace-relative target.
    Symlink,
    /// A Windows directory junction. Needs no privileges, but stores an absolute
    /// target, so it must be rebuilt if the workspace moves.
    Junction,
    /// A stub file containing a tool-specific include directive.
    Import,
}

impl Via {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Junction => "junction",
            Self::Import => "import",
        }
    }

    /// Whether this mechanism produces a real filesystem link, and therefore
    /// propagates edits, renames and deletions for free.
    pub const fn is_link(self) -> bool {
        matches!(self, Self::Symlink | Self::Junction)
    }
}

impl fmt::Display for Via {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which link primitives the host filesystem actually allows right now.
///
/// Probed once per run rather than assumed from the target triple: on Windows
/// symlink creation depends on Developer Mode or an elevated process, so two
/// machines with the same OS can differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSupport {
    pub symlink_file: bool,
    pub symlink_dir: bool,
    pub junction: bool,
}

impl LinkSupport {
    /// A host where every primitive works, as on a typical Unix system.
    pub const FULL: Self = Self {
        symlink_file: true,
        symlink_dir: true,
        junction: false,
    };

    /// A host where no link can be created; everything must fall back.
    pub const NONE: Self = Self {
        symlink_file: false,
        symlink_dir: false,
        junction: false,
    };

    /// Windows without symlink privileges: junctions still work for directories.
    pub const JUNCTION_ONLY: Self = Self {
        symlink_file: false,
        symlink_dir: false,
        junction: true,
    };

    /// The preferred mechanism for linking a node of the given kind, if any.
    ///
    /// Symlinks win when available because they are portable, relative and
    /// unambiguous to detect. Junctions are the directory-only fallback.
    pub const fn best_for(self, node: NodeKind) -> Option<Via> {
        match node {
            NodeKind::File if self.symlink_file => Some(Via::Symlink),
            NodeKind::Dir if self.symlink_dir => Some(Via::Symlink),
            // Junctions link directories only, which is exactly the case that
            // matters most: on Windows they need no elevation at all.
            NodeKind::Dir if self.junction => Some(Via::Junction),
            _ => None,
        }
    }
}

/// A filesystem entry as observed without following a trailing link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub node: NodeKind,
    /// Present when the entry is a symlink or junction.
    pub link: Option<LinkTarget>,
}

impl Entry {
    pub fn file() -> Self {
        Self {
            node: NodeKind::File,
            link: None,
        }
    }

    pub fn dir() -> Self {
        Self {
            node: NodeKind::Dir,
            link: None,
        }
    }

    pub fn link(node: NodeKind, target: LinkTarget) -> Self {
        Self {
            node,
            link: Some(target),
        }
    }

    /// Whether this entry holds real content rather than pointing elsewhere.
    pub fn is_concrete(&self) -> bool {
        self.link.is_none()
    }
}

/// Where a link points, as resolved by the filesystem adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Resolves to a path inside the workspace.
    Inside(RelPath),
    /// Resolves outside the workspace, or could not be expressed relatively.
    /// Kept as a display string only — the domain never acts on it.
    Outside(String),
}

impl fmt::Display for LinkTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inside(path) => write!(f, "{path}"),
            Self::Outside(raw) => f.write_str(raw),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_node_kinds_drive_link_choice() {
        assert_eq!(ResourceKind::Instructions.node(), NodeKind::File);
        assert_eq!(ResourceKind::Skills.node(), NodeKind::Dir);
    }

    #[test]
    fn symlinks_are_preferred_when_available() {
        assert_eq!(
            LinkSupport::FULL.best_for(NodeKind::Dir),
            Some(Via::Symlink)
        );
        assert_eq!(
            LinkSupport::FULL.best_for(NodeKind::File),
            Some(Via::Symlink)
        );
    }

    #[test]
    fn windows_without_privileges_still_links_directories() {
        // This is the case that makes the whole design viable on Windows: the
        // highest-value resource (skills) is a directory, and junctions have
        // never required elevation.
        assert_eq!(
            LinkSupport::JUNCTION_ONLY.best_for(NodeKind::Dir),
            Some(Via::Junction)
        );
        // Files have no link primitive here, so they must fall back to `import`.
        assert_eq!(LinkSupport::JUNCTION_ONLY.best_for(NodeKind::File), None);
    }

    #[test]
    fn no_support_yields_no_mechanism() {
        assert_eq!(LinkSupport::NONE.best_for(NodeKind::Dir), None);
        assert_eq!(LinkSupport::NONE.best_for(NodeKind::File), None);
    }

    #[test]
    fn only_real_links_propagate_edits() {
        assert!(Via::Symlink.is_link());
        assert!(Via::Junction.is_link());
        assert!(!Via::Import.is_link());
    }
}
