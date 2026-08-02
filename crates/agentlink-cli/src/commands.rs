//! Command implementations.

use agentlink_domain::layout::AGENTS_MD_TEMPLATE;
use agentlink_domain::model::Strategy;
use agentlink_domain::{Layout, ResourceKind, Via, Workspace, apply as execute, gitignore};
use anyhow::Result;

use crate::app::{App, rel};
use crate::render;
use crate::ui::{Ui, pad};

/// Process exit code signalling "work is pending or blocked", for CI.
pub const EXIT_PENDING: i32 = 2;

/// Creates the canonical layout and materialises every capability.
pub fn init(ui: Ui, dir: Option<std::path::PathBuf>) -> Result<i32> {
    let mut app = App::load(dir)?;

    ui.say(format!(
        "  {} {}",
        ui.bold("workspace"),
        ui.dim(&app.root.display().to_string())
    ));

    // Seeding a canonical file that a provider path could have been adopted into
    // would turn a clean adoption into an unresolvable two-sided merge. So seed
    // only what nothing is waiting to donate.
    for &resource in ResourceKind::ALL {
        let canonical = app.layout.canonical(resource).clone();
        if app.ws.probe(&canonical)?.is_some() {
            continue;
        }
        if !adoptable(&app, resource)?.is_empty() {
            continue;
        }
        match resource {
            ResourceKind::Instructions => app.ws.write(&canonical, AGENTS_MD_TEMPLATE)?,
            ResourceKind::Skills => app.ws.create_dir_all(&canonical)?,
        }
        ui.say(format!("  {} {}", ui.green("created"), canonical));
    }

    if !app.initialised {
        app.save_config()?;
        ui.say(format!(
            "  {} {}",
            ui.green("created"),
            agentlink_domain::layout::CONFIG_FILE
        ));
        app.initialised = true;
    }

    run_apply(ui, &mut app, false, false)
}

/// Materialises every capability that is not already correct.
pub fn apply(ui: Ui, dir: Option<std::path::PathBuf>, dry_run: bool, adopt: bool) -> Result<i32> {
    let mut app = App::load(dir)?;
    app.require_initialised()?;
    run_apply(ui, &mut app, dry_run, adopt)
}

/// Passes allowed before we conclude the plan is not converging.
///
/// Two are sufficient today — adoption creates the canonical resource, and the
/// pass after it links every provider that was waiting — but the bound is a
/// guard, not a target: failing to converge is a bug, and looping forever would
/// hide it.
const MAX_PASSES: usize = 8;

fn run_apply(ui: Ui, app: &mut App, dry_run: bool, adopt: bool) -> Result<i32> {
    let mut plan = app.plan(adopt)?;

    if dry_run {
        render::plan(ui, &plan);
        render::dry_run_note(ui, &plan);
        return Ok(exit_for(&plan));
    }

    // Run to a fixed point. Adopting content into the canonical layout makes that
    // resource exist for the first time, which unblocks every other provider that
    // was skipped for want of anything to share.
    let mut total = agentlink_domain::ApplyReport::default();
    let mut passes = 0;
    loop {
        let report = execute::apply(&plan, &app.ws, &mut app.lock)?;
        total.absorb(report);
        passes += 1;

        plan = app.plan(adopt)?;
        if plan.writes().count() == 0 || passes >= MAX_PASSES {
            break;
        }
    }

    app.save_lock()?;

    // The plan now describes the resting state, which is what the user wants to
    // see: what every agent ended up with, not the intermediate steps.
    render::plan(ui, &plan);

    if app.sync_gitignore(&plan)? {
        ui.say("");
        ui.say(format!("  {} .gitignore", ui.green("updated")));
    }

    render::report(ui, total);

    if plan.writes().count() > 0 {
        ui.say("");
        ui.say(format!(
            "  {} stopped after {MAX_PASSES} passes with work still pending — please report this",
            ui.red("error:")
        ));
        return Ok(1);
    }
    Ok(if total.blocked > 0 { EXIT_PENDING } else { 0 })
}

/// Shows what `apply` would do.
pub fn status(ui: Ui, dir: Option<std::path::PathBuf>, check: bool) -> Result<i32> {
    let app = App::load(dir)?;
    app.require_initialised()?;

    let plan = app.plan(false)?;
    ui.say(format!(
        "  {} {}",
        ui.bold("workspace"),
        ui.dim(&app.root.display().to_string())
    ));
    render::plan(ui, &plan);

    Ok(if check { exit_for(&plan) } else { 0 })
}

/// Moves agent-owned content into the canonical layout and links it back.
pub fn adopt(ui: Ui, dir: Option<std::path::PathBuf>, dry_run: bool) -> Result<i32> {
    let mut app = App::load(dir)?;
    app.require_initialised()?;
    run_apply(ui, &mut app, dry_run, true)
}

/// Removes everything agentlink created, leaving the canonical layout intact.
pub fn clean(ui: Ui, dir: Option<std::path::PathBuf>, dry_run: bool) -> Result<i32> {
    let mut app = App::load(dir)?;
    app.require_initialised()?;

    let entries = app.lock.entries.clone();
    if entries.is_empty() {
        ui.say(format!("  {}", ui.dim("nothing to remove")));
        return Ok(0);
    }

    let mut removed = 0;
    for entry in entries {
        let Some(found) = app.ws.probe(&entry.target)? else {
            app.lock.forget(&entry.provider, entry.resource);
            continue;
        };

        // Only remove what still looks like the thing we made. If a link became a
        // real directory, or a stub was rewritten by hand, it is the user's now.
        let removable = if entry.via.is_link() {
            found.link.is_some()
        } else {
            found.is_concrete() && app.ws.read(&entry.target)? == expected_stub(&app, &entry)
        };

        if !removable {
            ui.say(format!(
                "  {} {} {}",
                ui.yellow("kept"),
                entry.target,
                ui.dim("no longer matches what agentlink created")
            ));
            continue;
        }

        if !dry_run {
            if entry.via.is_link() {
                app.ws.remove_link(&entry.target, entry.resource.node())?;
            } else {
                app.ws.remove_file(&entry.target)?;
            }
            app.lock.forget(&entry.provider, entry.resource);
        }
        ui.say(format!("  {} {}", ui.green("removed"), entry.target));
        removed += 1;
    }

    if dry_run {
        ui.say("");
        ui.say(format!("  {}", ui.dim("dry run — nothing was removed")));
        return Ok(0);
    }

    app.save_lock()?;

    // Drop the managed .gitignore block along with the paths it covered.
    let gitignore_path = rel(".gitignore");
    if app.config.gitignore.manage && app.ws.probe(&gitignore_path)?.is_some() {
        let existing = app.ws.read(&gitignore_path)?;
        let updated = gitignore::update(&existing, &[]);
        if updated != existing {
            app.ws.write(&gitignore_path, &updated)?;
            ui.say(format!("  {} .gitignore", ui.green("updated")));
        }
    }

    ui.say("");
    ui.say(format!("  {}", ui.green(&format!("{removed} removed"))));
    Ok(0)
}

/// Prints the capability matrix for every known provider.
pub fn providers(ui: Ui, dir: Option<std::path::PathBuf>) -> Result<i32> {
    let app = App::load(dir)?;
    let layout = Layout::default();

    let width = app
        .registry
        .all()
        .iter()
        .map(|provider| provider.id.len())
        .max()
        .unwrap_or(12)
        .max(8);

    ui.say("");
    let header = ResourceKind::ALL
        .iter()
        .map(|resource| pad(resource.as_str(), 28))
        .collect::<String>();
    ui.say(format!(
        "  {} {}",
        ui.bold(&pad("provider", width)),
        ui.bold(header.trim_end())
    ));

    for provider in app.registry.all() {
        let mut row = String::new();
        for &resource in ResourceKind::ALL {
            let cell = match provider.capability(resource) {
                None => ui.dim(&pad("—", 28)),
                Some(capability) => {
                    let text = match capability.strategy {
                        Strategy::Native => format!("native  {}", layout.canonical(resource)),
                        Strategy::Link => format!("link    {}", capability.path),
                        Strategy::Import => format!("import  {}", capability.path),
                    };
                    let padded = pad(&text, 28);
                    if capability.strategy == Strategy::Native {
                        ui.dim(&padded)
                    } else {
                        padded
                    }
                }
            };
            row.push_str(&cell);
        }
        ui.say(format!("  {} {}", pad(&provider.id, width), row.trim_end()));
    }

    ui.say("");
    ui.say(format!(
        "  {}",
        ui.dim(
            "`native` means the tool already reads the canonical path — agentlink writes nothing."
        )
    ));
    ui.say(format!(
        "  {}",
        ui.dim("Add an agent by dropping a manifest into providers/ — no code changes needed.")
    ));
    Ok(0)
}

/// Diagnoses the environment and the workspace.
///
/// Each section returns the number of problems it found, so the exit code
/// reflects everything rather than only the last check.
pub fn doctor(ui: Ui, dir: Option<std::path::PathBuf>) -> Result<i32> {
    let app = App::load(dir)?;

    let problems = diagnose_environment(ui, &app)
        + diagnose_workspace(ui, &app)?
        + diagnose_links(ui, &app)?
        + diagnose_plan(ui, &app)?;

    ui.say("");
    if problems == 0 {
        ui.say(format!("  {}", ui.green("no problems found")));
        return Ok(0);
    }
    ui.say(format!(
        "  {}",
        ui.yellow(&format!("{problems} problems found"))
    ));
    Ok(EXIT_PENDING)
}

/// Which link primitives this host permits, and what that implies.
fn diagnose_environment(ui: Ui, app: &App) -> usize {
    ui.say("");
    ui.say(format!("  {}", ui.bold("environment")));

    let support = app.ws.support();
    if support.symlink_dir && support.symlink_file {
        check(ui, true, "symlinks", "available");
        return 0;
    }
    if support.junction {
        check(
            ui,
            true,
            "junctions",
            "available (directories link without elevation)",
        );
        ui.say(format!(
            "         {}",
            ui.dim(
                "symlinks are unavailable: enable Windows Developer Mode to link files too;\n         \
                 until then, file capabilities fall back to import stubs"
            )
        ));
        return 0;
    }

    check(
        ui,
        false,
        "links",
        "no link primitive is available on this host",
    );
    1
}

/// Configuration, canonical layout and git integration.
fn diagnose_workspace(ui: Ui, app: &App) -> Result<usize> {
    ui.say("");
    ui.say(format!("  {}", ui.bold("workspace")));

    let problems = usize::from(!app.initialised);
    check(
        ui,
        app.initialised,
        "config",
        if app.initialised {
            "found"
        } else {
            "missing — run `agentlink init`"
        },
    );

    for &resource in ResourceKind::ALL {
        let canonical = app.layout.canonical(resource);
        let present = app.ws.probe(canonical)?.is_some();
        check(
            ui,
            present,
            canonical.as_str(),
            if present {
                "present"
            } else {
                "not created yet"
            },
        );
    }

    let in_git = app.root.join(".git").exists();
    check(
        ui,
        true,
        "git",
        if in_git {
            "repository detected"
        } else {
            "not a git repository"
        },
    );

    if in_git && app.config.gitignore.manage {
        let gitignore_path = rel(".gitignore");
        let managed = match app.ws.probe(&gitignore_path)? {
            Some(_) => gitignore::is_managed(&app.ws.read(&gitignore_path)?),
            None => false,
        };
        check(
            ui,
            managed,
            ".gitignore",
            if managed {
                "managed block present"
            } else {
                "no managed block — run `agentlink apply`"
            },
        );
    }

    Ok(problems)
}

/// Stale junctions and links that drifted from what the lock recorded.
fn diagnose_links(ui: Ui, app: &App) -> Result<usize> {
    let mut problems = 0;

    // A junction records an absolute path, so copying or moving a workspace
    // leaves it silently pointing at the original — readable, plausible and
    // wrong. This is the one failure mode that would otherwise go unnoticed.
    let targets: Vec<_> = app
        .lock
        .entries
        .iter()
        .map(|entry| entry.target.clone())
        .collect();
    let stale = app.ws.stale_junctions(&targets);
    if !stale.is_empty() {
        problems += 1;
        ui.say("");
        ui.say(format!("  {}", ui.bold("stale links")));
        for (path, target) in stale {
            check(
                ui,
                false,
                path.as_str(),
                &format!("points outside this workspace, at {target}"),
            );
        }
        ui.say(format!(
            "         {}",
            ui.dim("run `agentlink apply` to rebuild them here")
        ));
    }

    // Paths the lock claims we own that no longer exist, or are no longer links.
    let mut drifted = Vec::new();
    for entry in &app.lock.entries {
        match app.ws.probe(&entry.target)? {
            None => drifted.push((entry.target.clone(), "missing")),
            Some(found) if entry.via.is_link() && found.link.is_none() => {
                drifted.push((entry.target.clone(), "replaced by real content"));
            }
            _ => {}
        }
    }
    if !drifted.is_empty() {
        ui.say("");
        ui.say(format!("  {}", ui.bold("drift")));
        for (path, why) in drifted {
            check(ui, false, path.as_str(), why);
        }
    }

    Ok(problems)
}

/// Work that `apply` would still do.
fn diagnose_plan(ui: Ui, app: &App) -> Result<usize> {
    if !app.initialised {
        return Ok(0);
    }

    let plan = app.plan(false)?;
    let blocked = plan.blocked().count();
    let pending = plan.writes().count();

    ui.say("");
    ui.say(format!("  {}", ui.bold("plan")));
    check(
        ui,
        pending == 0,
        "pending",
        &format!("{pending} capabilities need materialising"),
    );
    check(
        ui,
        blocked == 0,
        "blocked",
        &format!("{blocked} capabilities need a decision"),
    );

    Ok(usize::from(blocked > 0))
}

fn check(ui: Ui, ok: bool, label: &str, detail: &str) {
    let mark = if ok {
        ui.green("ok  ")
    } else {
        ui.yellow("warn")
    };
    ui.say(format!(
        "    {} {} {}",
        mark,
        pad(label, 18),
        ui.dim(detail)
    ));
}

fn exit_for(plan: &agentlink_domain::Plan) -> i32 {
    if plan.writes().count() > 0 || plan.blocked().count() > 0 {
        EXIT_PENDING
    } else {
        0
    }
}

/// Provider paths for `resource` that currently hold the only copy of content.
fn adoptable(app: &App, resource: ResourceKind) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for provider in app.providers()? {
        let Some(capability) = provider.capability(resource) else {
            continue;
        };
        if capability.strategy == Strategy::Native {
            continue;
        }
        if let Some(entry) = app.ws.probe(&capability.path)?
            && entry.is_concrete()
        {
            found.push(capability.path.to_string());
        }
    }
    Ok(found)
}

/// The stub contents agentlink would write for a lock entry, used by `clean` to
/// confirm a file is still ours before removing it.
///
/// Returns an empty string when the entry is not a stub or its provider is no
/// longer known, which makes the comparison in `clean` fail safe: an unmatched
/// file is kept, never deleted.
fn expected_stub(app: &App, entry: &agentlink_domain::lock::LockEntry) -> String {
    if entry.via != Via::Import {
        return String::new();
    }
    app.registry
        .get(&entry.provider)
        .and_then(|provider| provider.capability(entry.resource))
        .and_then(|capability| capability.import_body(app.layout.canonical(entry.resource)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentlink_domain::Plan;

    #[test]
    fn a_clean_plan_exits_zero() {
        assert_eq!(exit_for(&Plan::default()), 0);
    }
}
