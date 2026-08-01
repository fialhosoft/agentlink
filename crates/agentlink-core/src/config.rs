//! Workspace configuration (`.agentlink/config.toml`).

use serde::{Deserialize, Serialize};

/// Current on-disk format version.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,

    /// Providers to serve. Omit to serve every provider agentlink knows about,
    /// including ones added by future releases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,

    #[serde(default)]
    pub gitignore: Gitignore,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            providers: None,
            gitignore: Gitignore::default(),
        }
    }
}

/// How agentlink treats the paths it materialises with respect to git.
///
/// The default reflects the recommended posture: commit the canonical layout,
/// and let every developer's `agentlink apply` recreate their own links. Symlinks
/// committed to a repository silently degrade into plain text files for
/// teammates on Windows without `core.symlinks`, and junctions cannot be
/// represented in git at all. See `docs/adr/0005-git-posture.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gitignore {
    /// Whether agentlink maintains a marked block in `.gitignore` listing the
    /// provider paths it materialises.
    pub manage: bool,
}

impl Default for Gitignore {
    fn default() -> Self {
        Self { manage: true }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not parse .agentlink/config.toml: {0}")]
    Syntax(String),
    #[error(
        "unsupported config version {0} (this build understands {CONFIG_VERSION}); \
         upgrade agentlink"
    )]
    Version(u32),
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Config =
            toml::from_str(text).map_err(|err| ConfigError::Syntax(err.to_string()))?;
        if config.version != CONFIG_VERSION {
            return Err(ConfigError::Version(config.version));
        }
        Ok(config)
    }

    pub fn render(&self) -> String {
        let body = toml::to_string_pretty(self).expect("config is always serialisable");
        format!(
            "# agentlink configuration — https://github.com/agentlink-dev/agentlink\n\
             #\n\
             # `providers` lists the agents to serve. Remove the key entirely to\n\
             # serve every agent agentlink knows about, including ones added by\n\
             # future releases.\n\n{body}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = Config {
            version: CONFIG_VERSION,
            providers: Some(vec!["claude-code".into(), "cursor".into()]),
            gitignore: Gitignore { manage: false },
        };
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
    }

    #[test]
    fn defaults_round_trip() {
        let config = Config::default();
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
    }

    #[test]
    fn an_omitted_provider_list_means_every_provider() {
        let config = Config::parse("version = 1").unwrap();
        assert_eq!(config.providers, None);
        assert!(config.gitignore.manage);
    }

    #[test]
    fn rejects_unknown_keys_rather_than_ignoring_a_typo() {
        let err = Config::parse("version = 1\nproviderz = []").unwrap_err();
        assert!(matches!(err, ConfigError::Syntax(_)));
    }

    #[test]
    fn rejects_a_config_written_by_a_newer_agentlink() {
        assert!(matches!(
            Config::parse("version = 42").unwrap_err(),
            ConfigError::Version(42)
        ));
    }
}
