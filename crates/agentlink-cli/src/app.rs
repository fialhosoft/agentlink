//! Composition root: locating a workspace and wiring the domain to adapters.

use std::path::{Path, PathBuf};

use agentlink_core::layout::{CONFIG_FILE, LOCAL_PROVIDERS_DIR, LOCK_FILE};
use agentlink_core::{
    Config, Layout, Lock, Plan, Planner, Provider, Registry, RelPath, Workspace, gitignore, plan,
};
use agentlink_fs::RootedWorkspace;
use anyhow::{Context, Result, bail};

/// Everything a command needs to operate on one workspace.
#[derive(Debug)]
pub struct App {
    pub root: PathBuf,
    pub ws: RootedWorkspace,
    pub layout: Layout,
    pub registry: Registry,
    pub config: Config,
    pub lock: Lock,
    /// Whether `.agentlink/config.toml` exists, distinguishing a configured
    /// workspace from one that merely happens to contain agent files.
    pub initialised: bool,
}

impl App {
    /// Loads the workspace containing `dir`, or the current directory.
    pub fn load(dir: Option<PathBuf>) -> Result<Self> {
        let start = match dir {
            Some(dir) => dir,
            None => std::env::current_dir().context("cannot determine the current directory")?,
        };
        let root = discover_root(&start);
        let ws = RootedWorkspace::open(&root)
            .with_context(|| format!("cannot open workspace at {}", root.display()))?;
        let layout = Layout::default();

        let config_path = rel(CONFIG_FILE);
        let initialised = ws.probe(&config_path)?.is_some();
        let config = if initialised {
            Config::parse(&ws.read(&config_path)?)?
        } else {
            Config::default()
        };

        let lock_path = rel(LOCK_FILE);
        let lock = match ws.probe(&lock_path)? {
            Some(_) => Lock::parse(&ws.read(&lock_path)?)?,
            None => Lock::default(),
        };

        let locals = read_local_manifests(&ws)?;
        let registry = Registry::with_local(
            &layout,
            locals
                .iter()
                .map(|(name, text)| (name.as_str(), text.as_str())),
        )?;

        Ok(Self {
            root: ws.root().to_path_buf(),
            ws,
            layout,
            registry,
            config,
            lock,
            initialised,
        })
    }

    /// The providers this workspace serves.
    pub fn providers(&self) -> Result<Vec<&Provider>> {
        Ok(self.registry.select(self.config.providers.as_deref())?)
    }

    /// Decides what would happen, without touching anything.
    pub fn plan(&self, adopt: bool) -> Result<Plan> {
        let providers = self.providers()?;
        Ok(Planner::new(&self.layout, &self.lock, self.ws.support())
            .with_adopt(adopt)
            .plan(&providers, &self.ws)?)
    }

    pub fn save_lock(&self) -> Result<()> {
        self.ws.write(&rel(LOCK_FILE), &self.lock.render())?;
        Ok(())
    }

    pub fn save_config(&self) -> Result<()> {
        self.ws.write(&rel(CONFIG_FILE), &self.config.render())?;
        Ok(())
    }

    /// Rewrites the managed `.gitignore` block to cover every path agentlink
    /// materialises, returning whether the file changed.
    ///
    /// Only paths that actually exist as materialised artefacts are listed, so a
    /// provider that needs no work never adds noise to the file.
    pub fn sync_gitignore(&self, plan: &Plan) -> Result<bool> {
        if !self.config.gitignore.manage {
            return Ok(false);
        }
        let gitignore_path = rel(".gitignore");
        let existing = match self.ws.probe(&gitignore_path)? {
            Some(_) => self.ws.read(&gitignore_path)?,
            // Never create a .gitignore just to hold an empty block.
            None if plan.linked() == 0 && !has_materialised(plan) => return Ok(false),
            None => String::new(),
        };

        let mut entries: Vec<String> = plan
            .steps
            .iter()
            .filter(|step| {
                step.is_write() || matches!(step.outcome, plan::Outcome::UpToDate { .. })
            })
            .map(|step| step.target.to_string())
            .collect();
        entries.sort();
        entries.dedup();

        let updated = gitignore::update(&existing, &entries);
        if updated == existing {
            return Ok(false);
        }
        self.ws.write(&gitignore_path, &updated)?;
        Ok(true)
    }

    /// Fails when the workspace has never been initialised, pointing at the fix.
    pub fn require_initialised(&self) -> Result<()> {
        if !self.initialised {
            bail!(
                "no agentlink workspace here ({} not found)\n\
                 run `agentlink init` to create one",
                self.root.join(CONFIG_FILE).display()
            );
        }
        Ok(())
    }
}

fn has_materialised(plan: &Plan) -> bool {
    plan.steps
        .iter()
        .any(|step| step.is_write() || matches!(step.outcome, plan::Outcome::UpToDate { .. }))
}

/// Walks upward looking for a workspace marker.
///
/// `.agentlink/` wins over `.git/` so a configured subproject inside a monorepo
/// keeps its own layout instead of being absorbed by the outer repository.
fn discover_root(start: &Path) -> PathBuf {
    let mut current = Some(start);
    let mut git_root: Option<&Path> = None;

    while let Some(dir) = current {
        if dir.join(".agentlink").is_dir() {
            return dir.to_path_buf();
        }
        if git_root.is_none() && dir.join(".git").exists() {
            git_root = Some(dir);
        }
        current = dir.parent();
    }

    git_root.unwrap_or(start).to_path_buf()
}

fn read_local_manifests(ws: &RootedWorkspace) -> Result<Vec<(String, String)>> {
    let dir = ws.native(&rel(LOCAL_PROVIDERS_DIR));
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    let entries =
        std::fs::read_dir(&dir).with_context(|| format!("cannot read {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            manifests.push((name, text));
        }
    }
    // Deterministic order so overrides resolve identically on every machine.
    manifests.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(manifests)
}

/// Builds a [`RelPath`] from a constant known to be valid.
pub fn rel(path: &str) -> RelPath {
    RelPath::new(path).expect("constant paths are valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agentlink_marker_wins_over_an_outer_git_repository() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path();
        std::fs::create_dir_all(outer.join(".git")).unwrap();
        let inner = outer.join("packages").join("api");
        std::fs::create_dir_all(inner.join(".agentlink")).unwrap();

        assert_eq!(discover_root(&inner), inner);
    }

    #[test]
    fn falls_back_to_the_git_root() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path();
        std::fs::create_dir_all(outer.join(".git")).unwrap();
        let inner = outer.join("src").join("deep");
        std::fs::create_dir_all(&inner).unwrap();

        assert_eq!(discover_root(&inner), outer);
    }

    #[test]
    fn falls_back_to_the_starting_directory_outside_any_repository() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(discover_root(temp.path()), temp.path());
    }
}
