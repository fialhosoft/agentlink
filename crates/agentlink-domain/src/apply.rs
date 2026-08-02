//! Executing a plan.
//!
//! The executor is deliberately dull. Every decision was made during planning;
//! here we only carry it out and record what we did. That split is what lets
//! `agentlink status` promise exactly what `agentlink apply` will do.
//!
//! Two invariants hold for every operation below:
//!
//! * nothing is removed unless the lock says agentlink created it, and
//! * no removal is ever recursive.

use crate::lock::{Lock, LockEntry};
use crate::plan::{Outcome, Plan, Skip, Step};
use crate::workspace::{FsResult, Workspace};

/// What an [`apply`] run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub created: usize,
    pub rewritten: usize,
    pub relinked: usize,
    pub adopted: usize,
    /// Artefacts removed because their provider is no longer served.
    pub retired: usize,
    /// Capabilities that were already correct, or needed nothing to begin with.
    pub unchanged: usize,
    /// Capabilities awaiting a human decision.
    pub blocked: usize,
}

impl ApplyReport {
    pub fn changed(&self) -> usize {
        self.created + self.rewritten + self.relinked + self.adopted + self.retired
    }

    /// Accumulates a later pass into this one.
    ///
    /// `apply` runs to a fixed point: adopting content creates the canonical
    /// resource, which in turn unblocks every other provider waiting for it. The
    /// counts are summed, while `unchanged` and `blocked` are taken from the
    /// final pass because only that pass describes the resting state.
    pub fn absorb(&mut self, later: ApplyReport) {
        self.created += later.created;
        self.rewritten += later.rewritten;
        self.relinked += later.relinked;
        self.adopted += later.adopted;
        self.retired += later.retired;
        self.unchanged = later.unchanged;
        self.blocked = later.blocked;
    }
}

/// Executes a plan, updating `lock` to reflect what now exists.
///
/// Blocked steps are counted and skipped: the caller is responsible for
/// reporting them and choosing an exit code.
pub fn apply(plan: &Plan, ws: &dyn Workspace, lock: &mut Lock) -> FsResult<ApplyReport> {
    let mut report = ApplyReport::default();

    for step in &plan.steps {
        match &step.outcome {
            Outcome::Native | Outcome::UpToDate { .. } | Outcome::Skip(Skip::CanonicalMissing) => {
                report.unchanged += 1;
            }
            // The path diverged from what we created, so it belongs to whoever
            // changed it. Dropping the claim leaves the content untouched and
            // stops agentlink reporting a path it no longer has any say over.
            Outcome::Skip(Skip::Unmanaged) => {
                lock.forget(&step.provider_id, step.resource);
                report.unchanged += 1;
            }
            Outcome::Blocked(_) => {
                report.blocked += 1;
            }
            Outcome::Create { via } => {
                materialise(step, *via, ws)?;
                record(step, *via, lock);
                report.created += 1;
            }
            Outcome::Rewrite { via } => {
                materialise(step, *via, ws)?;
                record(step, *via, lock);
                report.rewritten += 1;
            }
            Outcome::Relink { via, .. } => {
                // Only ever reached for a link the lock says we created, so
                // removing it cannot destroy anything the user authored.
                ws.remove_link(&step.target, step.resource.node())?;
                materialise(step, *via, ws)?;
                record(step, *via, lock);
                report.relinked += 1;
            }
            Outcome::Adopt { via } => {
                adopt(step, ws)?;
                materialise(step, *via, ws)?;
                record(step, *via, lock);
                report.adopted += 1;
            }
            Outcome::Retire { via } => {
                // Only planned for a target the lock owns that still matches what
                // we created, so nothing here can be the user's work.
                if via.is_link() {
                    ws.remove_link(&step.target, step.resource.node())?;
                } else {
                    ws.remove_file(&step.target)?;
                }
                lock.forget(&step.provider_id, step.resource);
                report.retired += 1;
            }
        }
    }

    Ok(report)
}

/// Moves the provider's content into the canonical location.
///
/// The planner only emits [`Outcome::Adopt`] when the canonical side is absent or
/// an empty directory, so the rename below never overwrites content.
fn adopt(step: &Step, ws: &dyn Workspace) -> FsResult<()> {
    if ws.probe(&step.canonical)?.is_some() {
        ws.remove_empty_dir(&step.canonical)?;
    }
    if let Some(parent) = step.canonical.parent() {
        ws.create_dir_all(&parent)?;
    }
    ws.rename(&step.target, &step.canonical)
}

fn materialise(step: &Step, via: crate::model::Via, ws: &dyn Workspace) -> FsResult<()> {
    if via.is_link() {
        ws.link(via, step.resource.node(), &step.canonical, &step.target)
    } else {
        ws.write(
            &step.target,
            step.import_body.as_deref().unwrap_or_default(),
        )
    }
}

fn record(step: &Step, via: crate::model::Via, lock: &mut Lock) {
    lock.record(LockEntry {
        provider: step.provider_id.clone(),
        resource: step.resource,
        target: step.target.clone(),
        canonical: step.canonical.clone(),
        via,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::model::{NodeKind, ResourceKind, Via};
    use crate::path::RelPath;
    use crate::plan::Planner;
    use crate::provider::Provider;
    use crate::testing::FakeWorkspace;

    fn provider(toml_text: &str) -> Provider {
        let layout = Layout::default();
        crate::provider::parse("test.toml", toml_text, |kind| {
            layout.canonical(kind).clone()
        })
        .expect("valid manifest")
    }

    fn claude() -> Provider {
        provider(
            r#"
            schema = 1
            id = "claude-code"
            name = "Claude Code"

            [[capability]]
            resource = "instructions"
            strategy = "link"
            path = "CLAUDE.md"

            [capability.fallback]
            strategy = "import"
            template = "@{canonical}\n"

            [[capability]]
            resource = "skills"
            strategy = "link"
            path = ".claude/skills"
            "#,
        )
    }

    /// Plans and applies in one shot, mirroring what `agentlink apply` does.
    fn run(ws: &FakeWorkspace, lock: &mut Lock, adopt: bool) -> ApplyReport {
        let layout = Layout::default();
        let plan = Planner::new(&layout, lock, ws.support())
            .with_adopt(adopt)
            .plan(&[&claude()], ws)
            .expect("planning");
        apply(&plan, ws, lock).expect("apply")
    }

    #[test]
    fn creates_links_and_records_ownership() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        let mut lock = Lock::default();

        let report = run(&ws, &mut lock, false);

        assert_eq!(report.created, 2);
        assert_eq!(
            ws.link_target(".claude/skills").as_deref(),
            Some(".agents/skills")
        );
        assert_eq!(ws.link_target("CLAUDE.md").as_deref(), Some("AGENTS.md"));
        assert!(lock.owns(&RelPath::new(".claude/skills").unwrap()));
        assert!(lock.owns(&RelPath::new("CLAUDE.md").unwrap()));
    }

    #[test]
    fn applying_twice_changes_nothing() {
        // Idempotence is the precondition for running this from a git hook.
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        let mut lock = Lock::default();

        run(&ws, &mut lock, false);
        let before = ws.paths();
        let second = run(&ws, &mut lock, false);

        assert_eq!(second.changed(), 0);
        assert_eq!(second.unchanged, 2);
        assert_eq!(ws.paths(), before);
        assert_eq!(lock.entries.len(), 2);
    }

    #[test]
    fn writes_an_import_stub_where_files_cannot_be_linked() {
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        let mut lock = Lock::default();

        run(&ws, &mut lock, false);

        // Directory gets a junction; the file gets Claude Code's own import syntax.
        assert_eq!(
            ws.link_target(".claude/skills").as_deref(),
            Some(".agents/skills")
        );
        assert_eq!(ws.raw_file("CLAUDE.md").as_deref(), Some("@AGENTS.md\n"));
        assert_eq!(
            lock.find("claude-code", ResourceKind::Skills).unwrap().via,
            Via::Junction
        );
        assert_eq!(
            lock.find("claude-code", ResourceKind::Instructions)
                .unwrap()
                .via,
            Via::Import
        );
    }

    #[test]
    fn adoption_moves_existing_content_then_links_back() {
        // The onboarding story: a repo that only ever used Claude Code.
        let ws = FakeWorkspace::unix();
        ws.add_file(".claude/skills/review/SKILL.md", "---\nname: review\n---\n");
        ws.add_file("CLAUDE.md", "# my instructions");
        let mut lock = Lock::default();

        let report = run(&ws, &mut lock, true);

        assert_eq!(report.adopted, 2);
        // Content now lives in the canonical location...
        assert_eq!(
            ws.raw_file(".agents/skills/review/SKILL.md").as_deref(),
            Some("---\nname: review\n---\n")
        );
        assert_eq!(
            ws.raw_file("AGENTS.md").as_deref(),
            Some("# my instructions")
        );
        // ...and the original paths still resolve, so Claude Code is unaffected.
        assert_eq!(
            ws.link_target(".claude/skills").as_deref(),
            Some(".agents/skills")
        );
        assert_eq!(ws.link_target("CLAUDE.md").as_deref(), Some("AGENTS.md"));
        assert_eq!(
            ws.read(&RelPath::new("CLAUDE.md").unwrap()).unwrap(),
            "# my instructions"
        );
    }

    #[test]
    fn adoption_clears_an_empty_canonical_directory_first() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_file(".claude/skills/review/SKILL.md", "body");
        let mut lock = Lock::default();

        let report = run(&ws, &mut lock, true);

        assert_eq!(report.adopted, 1);
        assert_eq!(
            ws.raw_file(".agents/skills/review/SKILL.md").as_deref(),
            Some("body")
        );
    }

    #[test]
    fn blocked_steps_are_counted_and_leave_the_workspace_untouched() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# shared");
        ws.add_file("CLAUDE.md", "# hand written, do not touch");
        ws.add_dir(".agents/skills");
        let mut lock = Lock::default();

        let report = run(&ws, &mut lock, false);

        assert_eq!(report.blocked, 1);
        assert_eq!(
            ws.raw_file("CLAUDE.md").as_deref(),
            Some("# hand written, do not touch")
        );
        assert!(!lock.owns(&RelPath::new("CLAUDE.md").unwrap()));
    }

    #[test]
    fn relinking_replaces_only_links_we_created() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir("old/skills");
        ws.add_dir(".agents/skills");
        let mut lock = Lock::default();

        // Simulate a previous run that pointed at a different canonical path.
        ws.add_link(".claude/skills", NodeKind::Dir, "old/skills");
        lock.record(LockEntry {
            provider: "claude-code".into(),
            resource: ResourceKind::Skills,
            target: RelPath::new(".claude/skills").unwrap(),
            canonical: RelPath::new("old/skills").unwrap(),
            via: Via::Symlink,
        });

        let report = run(&ws, &mut lock, false);

        assert_eq!(report.relinked, 1);
        assert_eq!(
            ws.link_target(".claude/skills").as_deref(),
            Some(".agents/skills")
        );
        // The old target's content is untouched — we removed a link, not a tree.
        assert!(ws.exists("old/skills"));
    }

    #[test]
    fn a_stale_stub_is_rewritten_in_place() {
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_file("CLAUDE.md", "@OLD.md\n");
        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: "claude-code".into(),
            resource: ResourceKind::Instructions,
            target: RelPath::new("CLAUDE.md").unwrap(),
            canonical: RelPath::new("AGENTS.md").unwrap(),
            via: Via::Import,
        });

        let report = run(&ws, &mut lock, false);

        assert_eq!(report.rewritten, 1);
        assert_eq!(ws.raw_file("CLAUDE.md").as_deref(), Some("@AGENTS.md\n"));
    }

    /// Plans a retirement the way `agentlink apply` composes one, then executes it.
    fn run_retiring(ws: &FakeWorkspace, lock: &mut Lock, selected: &[&str]) -> ApplyReport {
        let registry = crate::registry::Registry::builtin(&Layout::default()).unwrap();
        let providers: Vec<&Provider> = selected
            .iter()
            .map(|id| registry.get(id).expect("known provider"))
            .collect();
        let steps = crate::plan::retire(lock, &providers, &registry, ws).expect("planning");
        apply(&Plan { steps }, ws, lock).expect("apply")
    }

    #[test]
    fn retiring_removes_the_link_and_drops_the_claim() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_file(".agents/skills/deploy/SKILL.md", "---\nname: deploy\n---\n");
        ws.add_link(".cursor/skills", NodeKind::Dir, ".agents/skills");
        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: "cursor".into(),
            resource: ResourceKind::Skills,
            target: RelPath::new(".cursor/skills").unwrap(),
            canonical: RelPath::new(".agents/skills").unwrap(),
            via: Via::Symlink,
        });

        let report = run_retiring(&ws, &mut lock, &["claude-code"]);

        assert_eq!(report.retired, 1);
        assert!(!ws.exists(".cursor/skills"));
        assert!(lock.entries.is_empty());
        // A link was removed, never the tree it pointed at.
        assert!(ws.exists(".agents/skills/deploy/SKILL.md"));
    }

    #[test]
    fn a_deselected_path_the_user_took_over_survives_and_stops_being_claimed() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_file(".cursor/skills/deploy/SKILL.md", "---\nname: deploy\n---\n");
        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: "cursor".into(),
            resource: ResourceKind::Skills,
            target: RelPath::new(".cursor/skills").unwrap(),
            canonical: RelPath::new(".agents/skills").unwrap(),
            via: Via::Symlink,
        });

        let report = run_retiring(&ws, &mut lock, &["claude-code"]);

        assert_eq!(report.retired, 0);
        assert!(ws.exists(".cursor/skills/deploy/SKILL.md"));
        // Dropping the claim is what keeps the next run from reporting it again.
        assert!(lock.entries.is_empty());
    }
}
