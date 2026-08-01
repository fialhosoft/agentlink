//! The canonical layout: the single source of truth on disk.
//!
//! The layout is deliberately *not* an agentlink invention. Every path here is
//! one that a significant share of the ecosystem already reads natively:
//!
//! | Canonical        | Read natively by                                     |
//! |------------------|------------------------------------------------------|
//! | `AGENTS.md`      | Codex, Cursor, Copilot, `OpenCode`, Antigravity, …   |
//! | `.agents/skills` | Antigravity (IDE/CLI/AGY), Codex                     |
//!
//! Choosing the layout most tools already understand is what keeps the number of
//! [`Strategy::Native`](crate::model::Strategy::Native) verdicts high, and every
//! native verdict is work that never has to happen. See
//! `docs/adr/0002-canonical-layout.md`.

use crate::model::{NodeKind, ResourceKind};
use crate::path::RelPath;

/// Directory holding agentlink's own state, relative to the workspace root.
pub const STATE_DIR: &str = ".agentlink";

/// User-editable configuration.
pub const CONFIG_FILE: &str = ".agentlink/config.toml";

/// Record of everything agentlink has materialised.
pub const LOCK_FILE: &str = ".agentlink/lock.toml";

/// Directory scanned for workspace-local provider manifests.
pub const LOCAL_PROVIDERS_DIR: &str = ".agentlink/providers";

/// The canonical layout of a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    instructions: RelPath,
    skills: RelPath,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            instructions: RelPath::new("AGENTS.md").expect("valid default"),
            skills: RelPath::new(".agents/skills").expect("valid default"),
        }
    }
}

impl Layout {
    /// The canonical path for a resource kind.
    pub fn canonical(&self, kind: ResourceKind) -> &RelPath {
        match kind {
            ResourceKind::Instructions => &self.instructions,
            ResourceKind::Skills => &self.skills,
        }
    }

    /// The node kind of a canonical resource.
    pub fn node(&self, kind: ResourceKind) -> NodeKind {
        kind.node()
    }

    /// Paths that `init` should create so the workspace has a source of truth.
    pub fn seeds(&self) -> impl Iterator<Item = (ResourceKind, &RelPath)> {
        ResourceKind::ALL
            .iter()
            .map(|&kind| (kind, self.canonical(kind)))
    }
}

/// The starter `AGENTS.md` written by `agentlink init` when none exists.
///
/// Kept intentionally short: this file is read by every agent on every task, so
/// bloat here is a tax paid on every single request.
pub const AGENTS_MD_TEMPLATE: &str = "\
# AGENTS.md

Shared instructions for every AI coding agent working in this repository.
Managed with [agentlink](https://github.com/agentlink-dev/agentlink) — edit this
file from any agent and the change is visible to all of them.

## Project

<!-- What this project is, in two or three sentences. -->

## Commands

<!-- Build, test and lint commands an agent should use. -->

## Conventions

<!-- Code style, patterns to follow, and anything that is off limits. -->
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_paths_match_the_ecosystem_standards() {
        let layout = Layout::default();
        assert_eq!(
            layout.canonical(ResourceKind::Instructions).as_str(),
            "AGENTS.md"
        );
        assert_eq!(
            layout.canonical(ResourceKind::Skills).as_str(),
            ".agents/skills"
        );
    }

    #[test]
    fn every_resource_kind_has_a_seed() {
        let layout = Layout::default();
        assert_eq!(layout.seeds().count(), ResourceKind::ALL.len());
    }
}
