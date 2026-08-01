//! Providers are data, not code.
//!
//! A provider — an AI coding agent that agentlink knows how to serve — is
//! described entirely by a TOML manifest. Adding support for a new agent is one
//! file in `providers/` plus a test fixture; no core change, no recompilation of
//! anyone's mental model, and no Rust knowledge required.
//!
//! Manifests are parsed with `deny_unknown_fields` so a typo in a community
//! contribution fails loudly at load time instead of silently doing nothing.

use serde::Deserialize;

use crate::model::{ResourceKind, Strategy};
use crate::path::{PathError, RelPath};

/// The manifest schema version this build understands.
pub const SCHEMA_VERSION: u32 = 1;

/// Placeholder replaced with the canonical path in an `import` template.
const CANONICAL_PLACEHOLDER: &str = "{canonical}";

/// Why a manifest was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("{source_name}: {message}")]
    Syntax {
        source_name: String,
        message: String,
    },

    #[error(
        "{source_name}: unsupported schema version {found} (this build supports {SCHEMA_VERSION})"
    )]
    Schema { source_name: String, found: u32 },

    #[error("{source_name}: provider id `{id}` must be lowercase alphanumeric with dashes")]
    InvalidId { source_name: String, id: String },

    #[error("{source_name}: invalid path for `{resource}`: {source}")]
    InvalidPath {
        source_name: String,
        resource: ResourceKind,
        #[source]
        source: PathError,
    },

    #[error("{source_name}: declares `{resource}` more than once")]
    DuplicateResource {
        source_name: String,
        resource: ResourceKind,
    },

    #[error(
        "{source_name}: `{resource}` is declared `native` at `{declared}`, but the canonical path is `{canonical}`"
    )]
    NativeMismatch {
        source_name: String,
        resource: ResourceKind,
        declared: RelPath,
        canonical: RelPath,
    },

    #[error(
        "{source_name}: `{resource}` uses `import` but its template omits `{CANONICAL_PLACEHOLDER}`"
    )]
    TemplateMissingPlaceholder {
        source_name: String,
        resource: ResourceKind,
    },

    #[error("{source_name}: `{resource}` declares a `native` fallback, which is never meaningful")]
    NativeFallback {
        source_name: String,
        resource: ResourceKind,
    },
}

/// A validated provider definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub homepage: Option<String>,
    pub docs: Option<String>,
    pub capabilities: Vec<Capability>,
}

impl Provider {
    /// The capability serving a given resource, if the provider declares one.
    pub fn capability(&self, resource: ResourceKind) -> Option<&Capability> {
        self.capabilities
            .iter()
            .find(|cap| cap.resource == resource)
    }
}

/// How one provider obtains one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub resource: ResourceKind,
    pub strategy: Strategy,
    /// Where this provider expects to find the resource.
    pub path: RelPath,
    pub note: Option<String>,
    /// Used when the preferred strategy is impossible on the host.
    pub fallback: Option<Fallback>,
}

/// A degraded but still correct way to serve a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fallback {
    pub strategy: Strategy,
    pub template: String,
}

impl Capability {
    /// Renders the `import` stub contents for this capability.
    ///
    /// Returns `None` when this capability has no import path at all. An inline
    /// `template` on an `import` capability is normalised into `fallback` at
    /// parse time, so there is exactly one place to read it from.
    pub fn import_body(&self, canonical: &RelPath) -> Option<String> {
        let fallback = self
            .fallback
            .as_ref()
            .filter(|f| f.strategy == Strategy::Import)?;
        let target = canonical.relative_to_dir(self.path.parent().as_ref());
        Some(fallback.template.replace(CANONICAL_PLACEHOLDER, &target))
    }

    /// Whether this capability can degrade to an `import` stub when the host
    /// cannot create the link it would prefer.
    pub fn has_import_fallback(&self) -> bool {
        self.fallback
            .as_ref()
            .is_some_and(|f| f.strategy == Strategy::Import)
    }
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema: u32,
    id: String,
    name: String,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    docs: Option<String>,
    #[serde(default, rename = "capability")]
    capabilities: Vec<RawCapability>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCapability {
    resource: ResourceKind,
    strategy: Strategy,
    path: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    fallback: Option<RawFallback>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFallback {
    strategy: Strategy,
    #[serde(default)]
    template: Option<String>,
}

/// Parses and validates a provider manifest.
///
/// `source_name` is used purely for diagnostics — typically the file name.
/// `canonical` supplies the canonical path per resource so that `native`
/// declarations can be checked against reality rather than trusted.
pub fn parse(
    source_name: &str,
    toml_text: &str,
    canonical: impl Fn(ResourceKind) -> RelPath,
) -> Result<Provider, ManifestError> {
    let raw: RawManifest = toml::from_str(toml_text).map_err(|err| ManifestError::Syntax {
        source_name: source_name.to_string(),
        message: err.to_string(),
    })?;

    if raw.schema != SCHEMA_VERSION {
        return Err(ManifestError::Schema {
            source_name: source_name.to_string(),
            found: raw.schema,
        });
    }
    if !is_valid_id(&raw.id) {
        return Err(ManifestError::InvalidId {
            source_name: source_name.to_string(),
            id: raw.id,
        });
    }

    let mut capabilities: Vec<Capability> = Vec::with_capacity(raw.capabilities.len());
    for cap in raw.capabilities {
        if capabilities
            .iter()
            .any(|seen| seen.resource == cap.resource)
        {
            return Err(ManifestError::DuplicateResource {
                source_name: source_name.to_string(),
                resource: cap.resource,
            });
        }

        let path = RelPath::new(&cap.path).map_err(|source| ManifestError::InvalidPath {
            source_name: source_name.to_string(),
            resource: cap.resource,
            source,
        })?;
        let canonical_path = canonical(cap.resource);

        // A `native` declaration is a factual claim about the tool: "it reads the
        // canonical path directly". If the declared path differs, the claim is
        // wrong and we would silently write nothing while the user believes they
        // are covered. Fail instead.
        if cap.strategy == Strategy::Native && path != canonical_path {
            return Err(ManifestError::NativeMismatch {
                source_name: source_name.to_string(),
                resource: cap.resource,
                declared: path,
                canonical: canonical_path,
            });
        }

        // Normalise both spellings of "there is an import stub" into one slot:
        // a capability whose primary strategy is `import` carries its template
        // inline, while a `link` capability carries it under `[capability.fallback]`.
        let fallback = match (cap.strategy, cap.template, cap.fallback) {
            (Strategy::Import, template, _) => Some(Fallback {
                strategy: Strategy::Import,
                template: template.unwrap_or_default(),
            }),
            (_, _, Some(raw_fallback)) => {
                if raw_fallback.strategy == Strategy::Native {
                    return Err(ManifestError::NativeFallback {
                        source_name: source_name.to_string(),
                        resource: cap.resource,
                    });
                }
                Some(Fallback {
                    strategy: raw_fallback.strategy,
                    template: raw_fallback.template.unwrap_or_default(),
                })
            }
            (_, _, None) => None,
        };

        // An import template that never mentions the canonical file produces an
        // inert stub: the tool reads it, finds no reference, and the user
        // silently gets nothing. Reject it at load time.
        if let Some(f) = &fallback
            && f.strategy == Strategy::Import
            && !f.template.contains(CANONICAL_PLACEHOLDER)
        {
            return Err(ManifestError::TemplateMissingPlaceholder {
                source_name: source_name.to_string(),
                resource: cap.resource,
            });
        }

        capabilities.push(Capability {
            resource: cap.resource,
            strategy: cap.strategy,
            path,
            note: cap.note,
            fallback,
        });
    }

    capabilities.sort_by_key(|cap| cap.resource);

    Ok(Provider {
        id: raw.id,
        name: raw.name,
        homepage: raw.homepage,
        docs: raw.docs,
        capabilities,
    })
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.ends_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    fn canonical() -> impl Fn(ResourceKind) -> RelPath {
        let layout = Layout::default();
        move |kind| layout.canonical(kind).clone()
    }

    fn parse_ok(text: &str) -> Provider {
        parse("test.toml", text, canonical()).expect("manifest should parse")
    }

    #[test]
    fn parses_a_link_capability() {
        let provider = parse_ok(
            r#"
            schema = 1
            id = "claude-code"
            name = "Claude Code"

            [[capability]]
            resource = "skills"
            strategy = "link"
            path = ".claude/skills"
            "#,
        );
        assert_eq!(provider.id, "claude-code");
        let cap = provider.capability(ResourceKind::Skills).unwrap();
        assert_eq!(cap.strategy, Strategy::Link);
        assert_eq!(cap.path.as_str(), ".claude/skills");
    }

    #[test]
    fn rejects_a_native_claim_that_does_not_match_the_canonical_path() {
        // Guards against a contributor marking a tool `native` at a path the tool
        // does not actually read, which would silently produce a no-op.
        let err = parse(
            "bogus.toml",
            r#"
            schema = 1
            id = "bogus"
            name = "Bogus"

            [[capability]]
            resource = "skills"
            strategy = "native"
            path = ".bogus/skills"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ManifestError::NativeMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_paths_escaping_the_workspace() {
        let err = parse(
            "evil.toml",
            r#"
            schema = 1
            id = "evil"
            name = "Evil"

            [[capability]]
            resource = "skills"
            strategy = "link"
            path = "../../../.ssh"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ManifestError::InvalidPath { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = parse(
            "typo.toml",
            r#"
            schema = 1
            id = "typo"
            name = "Typo"

            [[capability]]
            resource = "skills"
            strategy = "link"
            pathh = ".typo/skills"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(matches!(err, ManifestError::Syntax { .. }), "got {err:?}");
    }

    #[test]
    fn rejects_import_templates_that_ignore_the_canonical_path() {
        let err = parse(
            "inert.toml",
            r#"
            schema = 1
            id = "inert"
            name = "Inert"

            [[capability]]
            resource = "instructions"
            strategy = "link"
            path = "INERT.md"

            [capability.fallback]
            strategy = "import"
            template = "read the other file\n"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ManifestError::TemplateMissingPlaceholder { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn import_body_resolves_the_canonical_path_relative_to_the_stub() {
        let provider = parse_ok(
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
            "#,
        );
        let cap = provider.capability(ResourceKind::Instructions).unwrap();
        let layout = Layout::default();
        assert_eq!(
            cap.import_body(layout.canonical(ResourceKind::Instructions)),
            Some("@AGENTS.md\n".to_string())
        );
    }

    #[test]
    fn import_body_walks_up_from_nested_stubs() {
        let provider = parse_ok(
            r#"
            schema = 1
            id = "nested"
            name = "Nested"

            [[capability]]
            resource = "instructions"
            strategy = "import"
            path = ".config/nested/RULES.md"
            template = "See {canonical}\n"
            "#,
        );
        let cap = provider.capability(ResourceKind::Instructions).unwrap();
        let layout = Layout::default();
        assert_eq!(
            cap.import_body(layout.canonical(ResourceKind::Instructions)),
            Some("See ../../AGENTS.md\n".to_string())
        );
    }

    #[test]
    fn rejects_duplicate_resources() {
        let err = parse(
            "dupe.toml",
            r#"
            schema = 1
            id = "dupe"
            name = "Dupe"

            [[capability]]
            resource = "skills"
            strategy = "link"
            path = ".a/skills"

            [[capability]]
            resource = "skills"
            strategy = "link"
            path = ".b/skills"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateResource { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_future_schema_versions() {
        let err = parse(
            "future.toml",
            r#"
            schema = 999
            id = "future"
            name = "Future"
            "#,
            canonical(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ManifestError::Schema { found: 999, .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn validates_provider_ids() {
        assert!(is_valid_id("claude-code"));
        assert!(is_valid_id("codex"));
        assert!(is_valid_id("gemini2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-leading"));
        assert!(!is_valid_id("trailing-"));
        assert!(!is_valid_id("Upper"));
        assert!(!is_valid_id("double--dash"));
        assert!(!is_valid_id("under_score"));
    }
}
