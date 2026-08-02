//! The capability lattice: deciding what, if anything, to do.
//!
//! Planning is a pure function of observable state — the canonical layout, the
//! provider manifests, what the filesystem currently holds, what the lock says we
//! own, and which link primitives the host permits. Nothing is written here.
//! `agentlink status` renders a plan; `agentlink apply` renders the same plan and
//! then executes it. That symmetry is what makes the tool predictable.
//!
//! The rule the planner exists to enforce: **never destroy content agentlink did
//! not create.** Every ambiguous situation resolves to a [`Blocked`] outcome that
//! names the exact command to run, rather than to a guess.

use crate::layout::Layout;
use crate::lock::Lock;
use crate::model::{Entry, LinkSupport, LinkTarget, NodeKind, ResourceKind, Strategy, Via};
use crate::path::RelPath;
use crate::provider::{Capability, Provider};
use crate::workspace::{FsResult, Workspace};

/// What the planner decided for one provider/resource pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The tool reads the canonical path directly. Nothing to do, now or ever.
    Native,
    /// Already materialised correctly.
    UpToDate { via: Via },
    /// Nothing at the target: create it.
    Create { via: Via },
    /// An `import` stub we own exists but its contents are stale.
    Rewrite { via: Via },
    /// A link we own points somewhere else: repoint it.
    Relink { via: Via, current: LinkTarget },
    /// The target holds the only copy of this content: move it into the canonical
    /// location, then link back. This is the onboarding path for an existing repo.
    Adopt { via: Via },
    /// This provider is no longer served, and what we made for it is still ours:
    /// remove it and stop claiming it.
    Retire { via: Via },
    /// Nothing to do, for a benign reason.
    Skip(Skip),
    /// Needs a human decision. Never resolved by guessing.
    Blocked(Blocked),
}

/// Benign reasons for inaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The canonical resource does not exist yet, so there is nothing to share.
    CanonicalMissing,
    /// A deselected provider's path no longer holds what agentlink put there, so
    /// it is the user's now: the file stays and agentlink drops its claim.
    Unmanaged,
}

/// Situations that require the user to choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// The target holds real content and the canonical location is free.
    /// Running `agentlink adopt` would move it and link back.
    NeedsAdopt,
    /// Both the target and the canonical location hold content. Only a human can
    /// decide how to merge them.
    TargetOccupied,
    /// A link exists that agentlink did not create, pointing somewhere else.
    ForeignLink { current: LinkTarget },
    /// The host cannot create the required link and the provider declares no
    /// fallback.
    Unsupported { node: NodeKind },
}

/// One decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub provider_id: String,
    pub provider_name: String,
    pub resource: ResourceKind,
    pub canonical: RelPath,
    pub target: RelPath,
    pub outcome: Outcome,
    pub note: Option<String>,
    /// Exact bytes to write when this step materialises an `import` stub.
    ///
    /// Rendered during planning so that a [`Step`] fully describes its own
    /// execution: the executor needs nothing but the plan, which keeps `status`
    /// and `apply` provably in agreement about what will happen.
    pub import_body: Option<String>,
}

impl Step {
    /// Whether executing this step materialises something at the target.
    ///
    /// Deliberately excludes [`Outcome::Retire`]: callers use this to decide what
    /// a workspace now contains — a `.gitignore` block, for instance — and a path
    /// on its way out belongs in neither.
    pub fn is_write(&self) -> bool {
        matches!(
            self.outcome,
            Outcome::Create { .. }
                | Outcome::Rewrite { .. }
                | Outcome::Relink { .. }
                | Outcome::Adopt { .. }
        )
    }

    /// Whether executing this step takes something away.
    pub fn is_removal(&self) -> bool {
        matches!(self.outcome, Outcome::Retire { .. })
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self.outcome, Outcome::Blocked(_))
    }
}

/// A full set of decisions for a workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl Plan {
    /// Steps that would write to disk.
    pub fn writes(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|step| step.is_write())
    }

    /// Steps needing a human decision.
    pub fn blocked(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|step| step.is_blocked())
    }

    /// Steps that would remove something agentlink created.
    pub fn removals(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|step| step.is_removal())
    }

    /// How many capabilities require no work at all — the number this project
    /// exists to maximise.
    pub fn free(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step.outcome, Outcome::Native | Outcome::UpToDate { .. }))
            .count()
    }

    /// How many capabilities are served by a real filesystem link, and therefore
    /// propagate edits, renames and deletions with no further action.
    pub fn linked(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| match step.outcome {
                Outcome::UpToDate { via }
                | Outcome::Create { via }
                | Outcome::Relink { via, .. }
                | Outcome::Adopt { via } => via.is_link(),
                _ => false,
            })
            .count()
    }

    pub fn is_clean(&self) -> bool {
        self.steps
            .iter()
            .all(|step| !step.is_write() && !step.is_removal() && !step.is_blocked())
    }
}

/// Decides what to do, given the world as it is.
#[derive(Debug)]
pub struct Planner<'a> {
    layout: &'a Layout,
    lock: &'a Lock,
    support: LinkSupport,
    adopt: bool,
}

impl<'a> Planner<'a> {
    pub fn new(layout: &'a Layout, lock: &'a Lock, support: LinkSupport) -> Self {
        Self {
            layout,
            lock,
            support,
            adopt: false,
        }
    }

    /// Permits moving user content from a provider path into the canonical
    /// location. Off by default: adoption is the one operation that relocates
    /// data the user did not put there, so it must be asked for explicitly.
    #[must_use]
    pub fn with_adopt(mut self, adopt: bool) -> Self {
        self.adopt = adopt;
        self
    }

    pub fn plan(&self, providers: &[&Provider], ws: &dyn Workspace) -> FsResult<Plan> {
        let mut steps = Vec::new();
        for provider in providers {
            for &resource in ResourceKind::ALL {
                let Some(capability) = provider.capability(resource) else {
                    continue;
                };
                steps.push(self.step(provider, capability, ws)?);
            }
        }
        steps.sort_by(|a, b| {
            a.resource
                .cmp(&b.resource)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });
        Ok(Plan { steps })
    }

    fn step(
        &self,
        provider: &Provider,
        capability: &Capability,
        ws: &dyn Workspace,
    ) -> FsResult<Step> {
        let resource = capability.resource;
        let canonical = self.layout.canonical(resource).clone();
        let outcome = self.decide(capability, &canonical, ws)?;
        Ok(Step {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            resource,
            canonical: canonical.clone(),
            target: capability.path.clone(),
            outcome,
            note: capability.note.clone(),
            import_body: capability.import_body(&canonical),
        })
    }

    fn decide(
        &self,
        capability: &Capability,
        canonical: &RelPath,
        ws: &dyn Workspace,
    ) -> FsResult<Outcome> {
        let node = capability.resource.node();
        let canonical_entry = ws.probe(canonical)?;

        // `native` is a claim about the tool, already validated against the
        // canonical path at manifest load time. There is nothing to materialise.
        if capability.strategy == Strategy::Native {
            return Ok(match canonical_entry {
                Some(_) => Outcome::Native,
                None => Outcome::Skip(Skip::CanonicalMissing),
            });
        }

        let Some(via) = self.resolve_via(capability, node) else {
            return Ok(Outcome::Blocked(Blocked::Unsupported { node }));
        };

        let target_entry = ws.probe(&capability.path)?;

        match (canonical_entry, target_entry) {
            // The provider path holds the only copy. This is the common state of
            // a repository that has been using one agent and is now adding
            // agentlink, so it deserves a first-class path rather than an error.
            (None, Some(target)) if target.is_concrete() => Ok(self.adoption(via)),

            // Nothing to share yet: either the workspace is empty, or a dangling
            // link is waiting for canonical content to appear.
            (None, _) => Ok(Outcome::Skip(Skip::CanonicalMissing)),

            (Some(_), None) => Ok(Outcome::Create { via }),

            (Some(_), Some(target)) => self.reconcile(capability, canonical, via, &target, ws),
        }
    }

    fn reconcile(
        &self,
        capability: &Capability,
        canonical: &RelPath,
        via: Via,
        target: &Entry,
        ws: &dyn Workspace,
    ) -> FsResult<Outcome> {
        if via == Via::Import {
            return self.reconcile_import(capability, canonical, target, ws);
        }

        match &target.link {
            // Already pointing where it should. We deliberately do not rewrite a
            // junction into a symlink when privileges appear later: the link is
            // correct, and churn in a repository is worse than a suboptimal but
            // working mechanism.
            Some(LinkTarget::Inside(actual)) if actual == canonical => {
                Ok(Outcome::UpToDate { via })
            }
            Some(current) => Ok(if self.lock.owns(&capability.path) {
                Outcome::Relink {
                    via,
                    current: current.clone(),
                }
            } else {
                Outcome::Blocked(Blocked::ForeignLink {
                    current: current.clone(),
                })
            }),
            // Real content sits at the provider path while the canonical location
            // also exists. Safe to adopt only if the canonical side is empty.
            None => {
                if Self::canonical_is_free(canonical, ws)? {
                    Ok(self.adoption(via))
                } else {
                    Ok(Outcome::Blocked(Blocked::TargetOccupied))
                }
            }
        }
    }

    fn reconcile_import(
        &self,
        capability: &Capability,
        canonical: &RelPath,
        target: &Entry,
        ws: &dyn Workspace,
    ) -> FsResult<Outcome> {
        let expected = capability.import_body(canonical).unwrap_or_default();

        // A link where we expect a stub is not ours to interpret.
        if !target.is_concrete() {
            return Ok(match &target.link {
                Some(current) => Outcome::Blocked(Blocked::ForeignLink {
                    current: current.clone(),
                }),
                None => Outcome::Blocked(Blocked::TargetOccupied),
            });
        }

        let actual = ws.read(&capability.path)?;
        if actual == expected {
            return Ok(Outcome::UpToDate { via: Via::Import });
        }
        // The stub is one line of our own generated text. Rewriting it is safe
        // only if we wrote it; otherwise the file is the user's.
        Ok(if self.lock.owns(&capability.path) {
            Outcome::Rewrite { via: Via::Import }
        } else if Self::canonical_is_free(canonical, ws)? {
            self.adoption(Via::Import)
        } else {
            Outcome::Blocked(Blocked::TargetOccupied)
        })
    }

    /// Whether the canonical location can receive adopted content without
    /// overwriting anything.
    fn canonical_is_free(canonical: &RelPath, ws: &dyn Workspace) -> FsResult<bool> {
        Ok(match ws.probe(canonical)? {
            None => true,
            Some(entry) if entry.node == NodeKind::Dir && entry.is_concrete() => {
                ws.is_empty_dir(canonical)?
            }
            Some(_) => false,
        })
    }

    fn adoption(&self, via: Via) -> Outcome {
        if self.adopt {
            Outcome::Adopt { via }
        } else {
            Outcome::Blocked(Blocked::NeedsAdopt)
        }
    }

    /// Picks the mechanism: the provider's preferred strategy if the host allows
    /// it, otherwise its declared fallback.
    fn resolve_via(&self, capability: &Capability, node: NodeKind) -> Option<Via> {
        match capability.strategy {
            Strategy::Native => None,
            Strategy::Import => Some(Via::Import),
            Strategy::Link => match self.support.best_for(node) {
                Some(via) => Some(via),
                // No link primitive on this host. This is exactly the Windows
                // file case: junctions cannot link a file and symlinks need
                // privileges, so `CLAUDE.md` degrades to an `@AGENTS.md` stub.
                None if capability.has_import_fallback() => Some(Via::Import),
                None => None,
            },
        }
    }
}

/// Decides what to do with paths the lock claims, for providers that are no
/// longer served.
///
/// Narrowing the provider list would otherwise leave orphans behind: a
/// `.cursor/skills` junction nobody maintains, still listed in `.gitignore`.
/// These steps are part of the same plan as everything else, so `status`
/// announces a removal before `apply` performs one.
///
/// The safety rule is the one that governs the whole tool: a target is removed
/// only while it still *is* what agentlink created. A link replaced by a real
/// directory, or a stub someone edited, resolves to [`Skip::Unmanaged`] — the
/// content stays and agentlink simply stops claiming it.
pub fn retire(
    lock: &Lock,
    selected: &[&Provider],
    registry: &crate::registry::Registry,
    ws: &dyn Workspace,
) -> FsResult<Vec<Step>> {
    let mut steps = Vec::new();
    for entry in &lock.entries {
        if selected.iter().any(|p| p.id == entry.provider) {
            continue;
        }

        let outcome = match ws.probe(&entry.target)? {
            Some(found) if still_ours(entry, &found, registry, ws)? => {
                Outcome::Retire { via: entry.via }
            }
            // Already gone, or no longer what we made. Either way nothing is
            // deleted and the claim is dropped, so the next run stays quiet.
            _ => Outcome::Skip(Skip::Unmanaged),
        };

        steps.push(Step {
            provider_id: entry.provider.clone(),
            provider_name: registry
                .get(&entry.provider)
                .map_or_else(|| entry.provider.clone(), |p| p.name.clone()),
            resource: entry.resource,
            canonical: entry.canonical.clone(),
            target: entry.target.clone(),
            outcome,
            note: None,
            import_body: None,
        });
    }

    steps.sort_by(|a, b| {
        a.resource
            .cmp(&b.resource)
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });
    Ok(steps)
}

/// Whether what sits at a locked target is still the artefact agentlink made.
fn still_ours(
    entry: &crate::lock::LockEntry,
    found: &Entry,
    registry: &crate::registry::Registry,
    ws: &dyn Workspace,
) -> FsResult<bool> {
    if entry.via.is_link() {
        return Ok(found.link.is_some());
    }
    if !found.is_concrete() {
        return Ok(false);
    }
    // An import stub is one line of our own generated text, so comparing it to
    // what we would write today is an exact ownership test. A provider that has
    // since been removed from the registry leaves nothing to compare against,
    // which fails closed: the file is kept.
    let expected = registry
        .get(&entry.provider)
        .and_then(|provider| provider.capability(entry.resource))
        .and_then(|capability| capability.import_body(&entry.canonical));
    Ok(match expected {
        Some(expected) => ws.read(&entry.target)? == expected,
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::LockEntry;
    use crate::testing::FakeWorkspace;

    fn rel(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

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

    fn antigravity() -> Provider {
        provider(
            r#"
            schema = 1
            id = "antigravity"
            name = "Google Antigravity"

            [[capability]]
            resource = "instructions"
            strategy = "native"
            path = "AGENTS.md"

            [[capability]]
            resource = "skills"
            strategy = "native"
            path = ".agents/skills"
            "#,
        )
    }

    fn outcome_for(plan: &Plan, provider: &str, resource: ResourceKind) -> Outcome {
        plan.steps
            .iter()
            .find(|step| step.provider_id == provider && step.resource == resource)
            .unwrap_or_else(|| panic!("no step for {provider}/{resource}"))
            .outcome
            .clone()
    }

    fn plan_with(ws: &FakeWorkspace, lock: &Lock, providers: &[&Provider]) -> Plan {
        let layout = Layout::default();
        Planner::new(&layout, lock, ws.support())
            .plan(providers, ws)
            .expect("planning")
    }

    #[test]
    fn a_tool_reading_the_canonical_path_costs_nothing() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");

        let plan = plan_with(&ws, &Lock::default(), &[&antigravity()]);

        assert_eq!(
            outcome_for(&plan, "antigravity", ResourceKind::Instructions),
            Outcome::Native
        );
        assert_eq!(
            outcome_for(&plan, "antigravity", ResourceKind::Skills),
            Outcome::Native
        );
        // The entire point: a native provider triggers zero writes.
        assert_eq!(plan.writes().count(), 0);
        assert_eq!(plan.free(), 2);
    }

    #[test]
    fn missing_canonical_content_is_skipped_not_invented() {
        let ws = FakeWorkspace::unix();
        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Skip(Skip::CanonicalMissing)
        );
        assert_eq!(plan.writes().count(), 0);
    }

    #[test]
    fn creates_symlinks_on_a_host_that_supports_them() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Instructions),
            Outcome::Create { via: Via::Symlink }
        );
        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Create { via: Via::Symlink }
        );
        assert_eq!(plan.linked(), 2);
    }

    #[test]
    fn windows_without_privileges_junctions_directories_and_stubs_files() {
        // The decisive cross-platform case. Skills are a directory, so they get a
        // junction with no elevation. CLAUDE.md is a file with no available link
        // primitive, so it degrades to Claude Code's own `@` import syntax.
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Create { via: Via::Junction }
        );
        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Instructions),
            Outcome::Create { via: Via::Import }
        );
    }

    #[test]
    fn a_provider_without_a_fallback_is_reported_unsupported_not_silently_dropped() {
        let no_fallback = provider(
            r#"
            schema = 1
            id = "strict"
            name = "Strict"

            [[capability]]
            resource = "instructions"
            strategy = "link"
            path = "STRICT.md"
            "#,
        );
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");

        let plan = plan_with(&ws, &Lock::default(), &[&no_fallback]);

        assert_eq!(
            outcome_for(&plan, "strict", ResourceKind::Instructions),
            Outcome::Blocked(Blocked::Unsupported {
                node: NodeKind::File
            })
        );
    }

    #[test]
    fn an_existing_correct_link_is_left_alone() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        ws.add_link(".claude/skills", NodeKind::Dir, ".agents/skills");
        ws.add_link("CLAUDE.md", NodeKind::File, "AGENTS.md");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::UpToDate { via: Via::Symlink }
        );
        assert!(plan.is_clean());
        // Re-planning after apply must be a no-op: idempotence is what makes this
        // safe to wire into a git hook.
        assert_eq!(plan.writes().count(), 0);
    }

    #[test]
    fn a_foreign_link_is_never_repointed_without_asking() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        ws.add_dir("somewhere/else");
        ws.add_link(".claude/skills", NodeKind::Dir, "somewhere/else");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Blocked(Blocked::ForeignLink {
                current: LinkTarget::Inside(rel("somewhere/else"))
            })
        );
    }

    #[test]
    fn a_link_we_created_is_repointed_freely() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");
        ws.add_dir("old/skills");
        ws.add_link(".claude/skills", NodeKind::Dir, "old/skills");

        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: "claude-code".into(),
            resource: ResourceKind::Skills,
            target: rel(".claude/skills"),
            canonical: rel("old/skills"),
            via: Via::Symlink,
        });

        let plan = plan_with(&ws, &lock, &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Relink {
                via: Via::Symlink,
                current: LinkTarget::Inside(rel("old/skills"))
            }
        );
    }

    #[test]
    fn existing_provider_content_asks_before_moving_anything() {
        // A repo that has been using Claude Code and has no .agents/ yet.
        let ws = FakeWorkspace::unix();
        ws.add_dir(".claude/skills");
        ws.add_file(".claude/skills/review/SKILL.md", "---\nname: review\n---\n");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        // Default posture never relocates user data.
        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Blocked(Blocked::NeedsAdopt)
        );
    }

    #[test]
    fn adoption_moves_content_into_the_canonical_location_when_asked() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".claude/skills");
        ws.add_file(".claude/skills/review/SKILL.md", "---\nname: review\n---\n");

        let layout = Layout::default();
        let lock = Lock::default();
        let plan = Planner::new(&layout, &lock, ws.support())
            .with_adopt(true)
            .plan(&[&claude()], &ws)
            .expect("planning");

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Adopt { via: Via::Symlink }
        );
    }

    #[test]
    fn content_on_both_sides_is_a_merge_only_a_human_can_do() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_file(".agents/skills/deploy/SKILL.md", "---\nname: deploy\n---\n");
        ws.add_dir(".claude/skills");
        ws.add_file(".claude/skills/review/SKILL.md", "---\nname: review\n---\n");

        let layout = Layout::default();
        let lock = Lock::default();
        // Even with --adopt, we refuse: adopting would silently discard one side.
        let plan = Planner::new(&layout, &lock, ws.support())
            .with_adopt(true)
            .plan(&[&claude()], &ws)
            .expect("planning");

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Blocked(Blocked::TargetOccupied)
        );
    }

    #[test]
    fn an_empty_canonical_directory_still_accepts_adoption() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_dir(".claude/skills");
        ws.add_file(".claude/skills/review/SKILL.md", "---\nname: review\n---\n");

        let layout = Layout::default();
        let lock = Lock::default();
        let plan = Planner::new(&layout, &lock, ws.support())
            .with_adopt(true)
            .plan(&[&claude()], &ws)
            .expect("planning");

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Skills),
            Outcome::Adopt { via: Via::Symlink }
        );
    }

    #[test]
    fn a_correct_import_stub_is_up_to_date() {
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_file("CLAUDE.md", "@AGENTS.md\n");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Instructions),
            Outcome::UpToDate { via: Via::Import }
        );
    }

    #[test]
    fn a_stale_stub_we_own_is_rewritten() {
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_file("CLAUDE.md", "@OLD.md\n");

        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: "claude-code".into(),
            resource: ResourceKind::Instructions,
            target: rel("CLAUDE.md"),
            canonical: rel("AGENTS.md"),
            via: Via::Import,
        });

        let plan = plan_with(&ws, &lock, &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Instructions),
            Outcome::Rewrite { via: Via::Import }
        );
    }

    #[test]
    fn a_handwritten_claude_md_is_never_silently_replaced_by_a_stub() {
        // The single most important safety case: a user with real content in
        // CLAUDE.md and real content in AGENTS.md must not lose either.
        let ws = FakeWorkspace::windows_unprivileged();
        ws.add_file("AGENTS.md", "# shared rules");
        ws.add_file("CLAUDE.md", "# my carefully written Claude instructions");

        let plan = plan_with(&ws, &Lock::default(), &[&claude()]);

        assert_eq!(
            outcome_for(&plan, "claude-code", ResourceKind::Instructions),
            Outcome::Blocked(Blocked::TargetOccupied)
        );
    }

    #[test]
    fn steps_are_ordered_deterministically() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_dir(".agents/skills");

        let plan = plan_with(&ws, &Lock::default(), &[&claude(), &antigravity()]);

        let order: Vec<_> = plan
            .steps
            .iter()
            .map(|step| (step.resource, step.provider_id.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                (ResourceKind::Instructions, "antigravity"),
                (ResourceKind::Instructions, "claude-code"),
                (ResourceKind::Skills, "antigravity"),
                (ResourceKind::Skills, "claude-code"),
            ]
        );
    }

    fn registry() -> crate::registry::Registry {
        crate::registry::Registry::builtin(&Layout::default()).expect("shipped manifests")
    }

    fn locked(provider: &str, resource: ResourceKind, target: &str, via: Via) -> Lock {
        let mut lock = Lock::default();
        lock.record(LockEntry {
            provider: provider.to_string(),
            resource,
            target: rel(target),
            canonical: Layout::default().canonical(resource).clone(),
            via,
        });
        lock
    }

    fn retire_with(ws: &FakeWorkspace, lock: &Lock, selected: &[&str]) -> Vec<Step> {
        let registry = registry();
        let providers: Vec<&Provider> = selected
            .iter()
            .map(|id| registry.get(id).expect("known provider"))
            .collect();
        retire(lock, &providers, &registry, ws).expect("planning")
    }

    #[test]
    fn a_deselected_providers_link_is_retired() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_link(".cursor/skills", NodeKind::Dir, ".agents/skills");
        let lock = locked(
            "cursor",
            ResourceKind::Skills,
            ".cursor/skills",
            Via::Symlink,
        );

        let steps = retire_with(&ws, &lock, &["claude-code"]);

        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].target.as_str(), ".cursor/skills");
        assert_eq!(steps[0].outcome, Outcome::Retire { via: Via::Symlink });
    }

    #[test]
    fn a_still_selected_provider_is_never_retired() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_link(".cursor/skills", NodeKind::Dir, ".agents/skills");
        let lock = locked(
            "cursor",
            ResourceKind::Skills,
            ".cursor/skills",
            Via::Symlink,
        );

        assert!(retire_with(&ws, &lock, &["cursor", "claude-code"]).is_empty());
    }

    #[test]
    fn a_deselected_path_the_user_took_over_is_never_removed() {
        // The link became a real directory holding real skills. Whatever happened,
        // that content is not ours to delete just because a name left a list.
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_file(".cursor/skills/deploy/SKILL.md", "---\nname: deploy\n---\n");
        let lock = locked(
            "cursor",
            ResourceKind::Skills,
            ".cursor/skills",
            Via::Symlink,
        );

        let steps = retire_with(&ws, &lock, &["claude-code"]);

        assert_eq!(steps[0].outcome, Outcome::Skip(Skip::Unmanaged));
    }

    #[test]
    fn a_deselected_import_stub_is_retired_only_while_it_still_says_what_we_wrote() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules\n");
        ws.add_file("CLAUDE.md", "@AGENTS.md\n");
        let lock = locked(
            "claude-code",
            ResourceKind::Instructions,
            "CLAUDE.md",
            Via::Import,
        );

        let steps = retire_with(&ws, &lock, &["antigravity"]);
        assert_eq!(steps[0].outcome, Outcome::Retire { via: Via::Import });

        ws.add_file("CLAUDE.md", "@AGENTS.md\n\nAlso: never force push.\n");
        let steps = retire_with(&ws, &lock, &["antigravity"]);
        assert_eq!(steps[0].outcome, Outcome::Skip(Skip::Unmanaged));
    }

    #[test]
    fn a_claim_on_something_already_gone_is_dropped_rather_than_repeated() {
        let ws = FakeWorkspace::unix();
        let lock = locked(
            "cursor",
            ResourceKind::Skills,
            ".cursor/skills",
            Via::Symlink,
        );

        let steps = retire_with(&ws, &lock, &["claude-code"]);

        assert_eq!(steps[0].outcome, Outcome::Skip(Skip::Unmanaged));
    }

    #[test]
    fn a_retirement_is_a_removal_not_a_write() {
        // `.gitignore` is built from writes, so a path on its way out must not
        // count as one.
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        ws.add_link(".cursor/skills", NodeKind::Dir, ".agents/skills");
        let lock = locked(
            "cursor",
            ResourceKind::Skills,
            ".cursor/skills",
            Via::Symlink,
        );

        let step = &retire_with(&ws, &lock, &["claude-code"])[0];

        assert!(step.is_removal());
        assert!(!step.is_write());
    }
}
