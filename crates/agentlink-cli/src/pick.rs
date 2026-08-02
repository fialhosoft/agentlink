//! Asking which agents this repository actually uses.
//!
//! The answer is the difference between a repository that grows a `.cursor/` it
//! never asked for and one that stays exactly as wide as the team using it.

use agentlink_domain::{Provider, Registry, ResourceKind, Strategy};
use anyhow::{Context, Result};
use dialoguer::MultiSelect;
use dialoguer::theme::ColorfulTheme;

use crate::ui::{Ui, pad};

/// Presents every known agent with `checked` preselected.
///
/// Returns `None` when the user aborts with Esc or Ctrl-C, which callers must
/// treat as "change nothing".
pub fn providers(ui: Ui, registry: &Registry, checked: &[String]) -> Result<Option<Vec<String>>> {
    let width = registry
        .all()
        .iter()
        .map(|provider| provider.id.len())
        .max()
        .unwrap_or(8);

    let items: Vec<(String, bool)> = registry
        .all()
        .iter()
        .map(|provider| {
            (
                format!("{} {}", pad(&provider.id, width), hint(provider)),
                checked.contains(&provider.id),
            )
        })
        .collect();

    ui.say("");
    let picked = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Which agents does this repository use? (space toggles, enter confirms)")
        .items_checked(items)
        .interact_opt()
        .context("cannot read the selection from this terminal")?;

    Ok(picked.map(|indices| {
        indices
            .into_iter()
            .map(|index| registry.all()[index].id.clone())
            .collect()
    }))
}

/// One line explaining what selecting this agent would cost.
///
/// A fully native agent creates nothing at all, and saying so is the only way a
/// user can tell that checking it is free — nothing on disk ever reveals it.
fn hint(provider: &Provider) -> String {
    let paths: Vec<String> = ResourceKind::ALL
        .iter()
        .filter_map(|&resource| provider.capability(resource))
        .filter(|capability| capability.strategy != Strategy::Native)
        .map(|capability| capability.path.to_string())
        .collect();

    if paths.is_empty() {
        return format!(
            "{} — reads the canonical layout, costs nothing",
            provider.name
        );
    }
    format!("{} — {}", provider.name, paths.join(", "))
}
