//! Turning a [`Plan`] into something a human can act on.
//!
//! Every blocked outcome renders the exact command that resolves it. A tool that
//! refuses to guess owes the user a next step.

use agentlink_domain::plan::{Blocked, Outcome, Plan, Skip, Step};
use agentlink_domain::{ResourceKind, Via};

use crate::ui::{Ui, pad};

const PROVIDER_WIDTH: usize = 16;
const VERB_WIDTH: usize = 9;

/// Renders the full plan, grouped by the canonical resource it shares.
pub fn plan(ui: Ui, plan: &Plan) {
    if plan.steps.is_empty() {
        ui.say(format!("  {}", ui.dim("no providers selected")));
        return;
    }

    for &resource in ResourceKind::ALL {
        let steps: Vec<&Step> = plan
            .steps
            .iter()
            .filter(|step| step.resource == resource)
            .collect();
        if steps.is_empty() {
            continue;
        }

        let canonical = &steps[0].canonical;
        ui.say("");
        ui.say(format!(
            "  {} {}",
            ui.bold(canonical.as_str()),
            ui.dim(&format!("({resource})"))
        ));

        for step in steps {
            let (verb, detail) = describe(ui, step);
            ui.say(format!(
                "    {} {} {}",
                pad(&step.provider_id, PROVIDER_WIDTH),
                pad(&verb, VERB_WIDTH),
                detail
            ));
        }
    }

    summary(ui, plan);
    blockers(ui, plan);
}

fn describe(ui: Ui, step: &Step) -> (String, String) {
    match &step.outcome {
        Outcome::Native => (
            ui.dim("native"),
            ui.dim(&format!(
                "reads {} directly — nothing to do",
                step.canonical
            )),
        ),
        Outcome::UpToDate { via } => (
            ui.green("ok"),
            ui.dim(&format!("{} {}", step.target, mechanism(*via))),
        ),
        Outcome::Create { via } => (
            ui.cyan("create"),
            format!("{} {}", step.target, ui.dim(mechanism(*via))),
        ),
        Outcome::Rewrite { via } => (
            ui.cyan("rewrite"),
            format!("{} {}", step.target, ui.dim(mechanism(*via))),
        ),
        Outcome::Relink { via, current } => (
            ui.cyan("relink"),
            format!(
                "{} {}",
                step.target,
                ui.dim(&format!("was → {current}, now {}", mechanism(*via)))
            ),
        ),
        Outcome::Adopt { via } => (
            ui.yellow("adopt"),
            format!(
                "{} {}",
                step.target,
                ui.dim(&format!(
                    "→ moves into {}, then {}",
                    step.canonical,
                    mechanism(*via)
                ))
            ),
        ),
        Outcome::Retire { via } => (
            ui.yellow("retire"),
            format!(
                "{} {}",
                step.target,
                ui.dim(&format!("{} — no longer selected", mechanism(*via)))
            ),
        ),
        Outcome::Skip(Skip::CanonicalMissing) => (
            ui.dim("skip"),
            ui.dim(&format!("nothing at {} yet", step.canonical)),
        ),
        Outcome::Skip(Skip::Unmanaged) => (
            ui.dim("kept"),
            ui.dim(&format!(
                "{} no longer matches what agentlink created — it is yours now",
                step.target
            )),
        ),
        Outcome::Blocked(reason) => (ui.red("blocked"), blocked_detail(ui, step, reason)),
    }
}

fn blocked_detail(ui: Ui, step: &Step, reason: &Blocked) -> String {
    match reason {
        Blocked::NeedsAdopt => format!(
            "{} {}",
            step.target,
            ui.dim("holds the only copy — run `agentlink adopt` to share it")
        ),
        Blocked::TargetOccupied => format!(
            "{} {}",
            step.target,
            ui.dim(&format!(
                "and {} both have content — merge them by hand",
                step.canonical
            ))
        ),
        Blocked::ForeignLink { current } => format!(
            "{} {}",
            step.target,
            ui.dim(&format!(
                "already links to {current}, and agentlink did not create it"
            ))
        ),
        Blocked::Unsupported { node } => format!(
            "{} {}",
            step.target,
            ui.dim(&format!(
                "no way to link a {node:?} here, and this provider declares no fallback"
            ))
            .to_lowercase()
        ),
    }
}

fn mechanism(via: Via) -> &'static str {
    match via {
        Via::Symlink => "(symlink)",
        Via::Junction => "(junction)",
        Via::Import => "(import stub)",
    }
}

fn summary(ui: Ui, plan: &Plan) {
    let free = plan.free();
    let writes = plan.writes().count();
    let removals = plan.removals().count();
    // A retirement is a capability leaving, not one being served, so it is
    // counted beside the total rather than inside it.
    let total = plan.steps.len() - removals;
    let blocked = plan.blocked().count();

    ui.say("");
    let mut parts = vec![
        format!("{total} capabilities"),
        ui.dim(&format!("{free} need nothing")),
    ];
    if writes > 0 {
        parts.push(ui.cyan(&format!("{writes} to materialise")));
    }
    if removals > 0 {
        parts.push(ui.yellow(&format!("{removals} to retire")));
    }
    if blocked > 0 {
        parts.push(ui.red(&format!("{blocked} blocked")));
    }
    // The number that distinguishes agentlink from a generator: content is never
    // duplicated, so this is structurally always zero.
    parts.push(ui.dim("0 files copied"));

    ui.say(format!("  {}", parts.join(&ui.dim(" · "))));
}

fn blockers(ui: Ui, plan: &Plan) {
    let blocked: Vec<&Step> = plan.blocked().collect();
    if blocked.is_empty() {
        return;
    }

    let needs_adopt = blocked
        .iter()
        .any(|step| matches!(step.outcome, Outcome::Blocked(Blocked::NeedsAdopt)));

    ui.say("");
    if needs_adopt {
        ui.say(format!(
            "  {} some agent directories hold content that is not shared yet.",
            ui.yellow("note:")
        ));
        ui.say(format!(
            "        {}",
            ui.dim("`agentlink adopt` moves each into the canonical layout and links it back,")
        ));
        ui.say(format!(
            "        {}",
            ui.dim("so every agent keeps reading its usual path.")
        ));
    }
}

/// Explains that a dry run shows only the next pass.
///
/// Adoption creates a canonical resource that did not exist, so the providers
/// currently skipped for want of content become linkable immediately afterwards.
/// Saying so avoids the impression that they were left out.
pub fn dry_run_note(ui: Ui, plan: &Plan) {
    let adopting = plan
        .steps
        .iter()
        .filter(|step| matches!(step.outcome, Outcome::Adopt { .. }))
        .count();
    let waiting = plan
        .steps
        .iter()
        .filter(|step| matches!(step.outcome, Outcome::Skip(Skip::CanonicalMissing)))
        .count();

    ui.say("");
    if adopting > 0 && waiting > 0 {
        ui.say(format!(
            "  {}",
            ui.dim(&format!(
                "adopting creates the canonical layout, after which the {waiting} skipped \
                 capabilities link too"
            ))
        ));
    }
    ui.say(format!("  {}", ui.dim("dry run — nothing was written")));
}

/// Renders what an apply actually did.
pub fn report(ui: Ui, report: agentlink_domain::ApplyReport) {
    let mut parts = Vec::new();
    if report.created > 0 {
        parts.push(ui.green(&format!("{} created", report.created)));
    }
    if report.adopted > 0 {
        parts.push(ui.green(&format!("{} adopted", report.adopted)));
    }
    if report.relinked > 0 {
        parts.push(ui.green(&format!("{} relinked", report.relinked)));
    }
    if report.rewritten > 0 {
        parts.push(ui.green(&format!("{} rewritten", report.rewritten)));
    }
    if report.retired > 0 {
        parts.push(ui.green(&format!("{} retired", report.retired)));
    }
    if report.blocked > 0 {
        parts.push(ui.red(&format!("{} blocked", report.blocked)));
    }
    if parts.is_empty() {
        parts.push(ui.dim("already up to date"));
    }

    ui.say("");
    ui.say(format!("  {}", parts.join(&ui.dim(" · "))));
}
