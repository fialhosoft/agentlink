//! Which agents this repository shows evidence of already using.
//!
//! Evidence is deliberately narrow: only a path a provider *owns* counts. A
//! `native` capability reads the canonical layout, so `AGENTS.md` existing says
//! nothing about whether anyone here runs Codex — it would preselect every
//! native agent for everybody, which is the noise this exists to avoid.

use crate::model::Strategy;
use crate::provider::Provider;
use crate::workspace::{FsResult, Workspace};

/// Ids of providers with content of their own already in the workspace.
pub fn evidence(providers: &[&Provider], ws: &dyn Workspace) -> FsResult<Vec<String>> {
    let mut found = Vec::new();
    for provider in providers {
        if has_evidence(provider, ws)? {
            found.push(provider.id.clone());
        }
    }
    Ok(found)
}

fn has_evidence(provider: &Provider, ws: &dyn Workspace) -> FsResult<bool> {
    for capability in &provider.capabilities {
        if capability.strategy == Strategy::Native {
            continue;
        }
        if ws.probe(&capability.path)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::registry::Registry;
    use crate::testing::FakeWorkspace;

    fn registry() -> Registry {
        Registry::builtin(&Layout::default()).unwrap()
    }

    fn ids(registry: &Registry, ws: &FakeWorkspace) -> Vec<String> {
        let providers: Vec<&Provider> = registry.all().iter().collect();
        evidence(&providers, ws).unwrap()
    }

    #[test]
    fn a_repository_that_only_ever_used_claude_code_detects_only_it() {
        let ws = FakeWorkspace::unix();
        ws.add_file("CLAUDE.md", "# rules\n");
        ws.add_dir(".claude/skills");

        assert_eq!(ids(&registry(), &ws), vec!["claude-code".to_string()]);
    }

    #[test]
    fn a_fully_native_provider_never_counts_as_evidence() {
        // Antigravity and Codex read the canonical layout directly, so nothing on
        // disk can distinguish "this repo uses them" from "this repo uses
        // agentlink at all".
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules\n");
        ws.add_dir(".agents/skills");

        assert!(ids(&registry(), &ws).is_empty());
    }

    #[test]
    fn an_empty_workspace_detects_nothing() {
        assert!(ids(&registry(), &FakeWorkspace::unix()).is_empty());
    }

    #[test]
    fn a_link_at_a_provider_path_is_still_evidence() {
        // A link there means a previous agentlink run served this agent, which is
        // exactly the selection we want to offer back unchanged.
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_link(
            ".cursor/skills",
            crate::model::NodeKind::Dir,
            ".agents/skills",
        );

        assert_eq!(ids(&registry(), &ws), vec!["cursor".to_string()]);
    }
}
