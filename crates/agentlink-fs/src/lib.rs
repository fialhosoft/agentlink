//! The filesystem adapter.
//!
//! This crate is the only place that knows about native paths, separators,
//! reparse points and platform privileges. It implements
//! [`agentlink_domain::Workspace`] against a real rooted directory so
//! the domain can stay a pure function of observable state.
//!
//! Three platform facts shape everything here:
//!
//! * **Windows junctions never require elevation.** `mklink /J` has always been
//!   unprivileged. Since the highest-value resource — skills — is a *directory*,
//!   agentlink can link it on any Windows machine out of the box.
//! * **Windows symlinks do require elevation or Developer Mode.** Two machines
//!   running the same Windows build can differ, so support is *probed at runtime*
//!   rather than inferred from the target triple.
//! * **Junctions store an absolute target; symlinks can store a relative one.**
//!   Symlinks are therefore preferred: a relative target keeps the workspace
//!   movable. Junctions are the directory-only fallback, and
//!   [`RootedWorkspace::stale_junctions`] detects the ones a move left behind.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use agentlink_domain::model::{Entry, LinkSupport, LinkTarget, NodeKind, Via};
use agentlink_domain::path::RelPath;
use agentlink_domain::workspace::{FsError, FsResult, Workspace};

/// A real directory, viewed through workspace-relative paths.
#[derive(Debug, Clone)]
pub struct RootedWorkspace {
    root: PathBuf,
    support: LinkSupport,
}

impl RootedWorkspace {
    /// Opens `root`, probing which link primitives this host currently permits.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        // Canonicalising resolves `..` and any links on the way in, so link
        // targets can be compared against a stable root. The `\\?\` prefix
        // Windows adds is stripped: it would otherwise surface in every message
        // agentlink prints.
        let root = fs::canonicalize(&root).map_or(root, |resolved| strip_verbatim(&resolved));
        Ok(Self {
            root,
            support: probe_support(),
        })
    }

    /// Opens `root` with a fixed capability set, for tests that need to exercise
    /// a fallback path on a host that would not otherwise take it.
    pub fn with_support(root: impl Into<PathBuf>, support: LinkSupport) -> Self {
        Self {
            root: root.into(),
            support,
        }
    }

    /// The absolute root of this workspace.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Translates a workspace-relative path into a native one.
    pub fn native(&self, path: &RelPath) -> PathBuf {
        let mut native = self.root.clone();
        for segment in path.segments() {
            native.push(segment);
        }
        native
    }

    /// Links whose stored target no longer resolves inside this workspace.
    ///
    /// Junctions record an absolute path, so moving or copying a workspace leaves
    /// them pointing at the old location — silently, and with the old content
    /// still readable. `agentlink doctor` uses this to catch that case.
    pub fn stale_junctions(&self, candidates: &[RelPath]) -> Vec<(RelPath, String)> {
        candidates
            .iter()
            .filter_map(|path| match self.probe(path) {
                Ok(Some(Entry {
                    link: Some(LinkTarget::Outside(target)),
                    ..
                })) => Some((path.clone(), target)),
                _ => None,
            })
            .collect()
    }

    /// Converts a raw link target into one the domain can compare.
    fn interpret_target(&self, link: &RelPath, raw: &Path) -> LinkTarget {
        let resolved = if raw.is_absolute() {
            strip_verbatim(raw)
        } else {
            // Relative targets resolve against the directory holding the link.
            let mut base = match link.parent() {
                Some(parent) => self.native(&parent),
                None => self.root.clone(),
            };
            base.push(raw);
            base
        };

        let normalised = normalise(&resolved);
        let root = normalise(&strip_verbatim(&self.root));

        match normalised.strip_prefix(&root) {
            Ok(relative) => {
                let text = relative.to_string_lossy().replace('\\', "/");
                match RelPath::new(&text) {
                    Ok(rel) => LinkTarget::Inside(rel),
                    // The link resolves to the workspace root itself, which no
                    // capability can legitimately target.
                    Err(_) => LinkTarget::Outside(normalised.to_string_lossy().into_owned()),
                }
            }
            Err(_) => LinkTarget::Outside(normalised.to_string_lossy().into_owned()),
        }
    }
}

impl Workspace for RootedWorkspace {
    fn probe(&self, path: &RelPath) -> FsResult<Option<Entry>> {
        let native = self.native(path);
        let meta = match fs::symlink_metadata(&native) {
            Ok(meta) => meta,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(FsError::new("inspect", path, err)),
        };

        if !meta.file_type().is_symlink() {
            return Ok(Some(if meta.is_dir() {
                Entry::dir()
            } else {
                Entry::file()
            }));
        }

        // `is_symlink()` is true for both symlinks and junctions on Windows, and
        // `read_link` understands both, so links are handled uniformly. The
        // planner deliberately does not care which primitive was used: a link
        // pointing at the right place is correct, and rewriting a working
        // junction into a symlink would be churn for no benefit.
        let raw =
            fs::read_link(&native).map_err(|err| FsError::new("read the link at", path, err))?;
        let node = link_node_kind(&native, &meta);
        Ok(Some(Entry::link(node, self.interpret_target(path, &raw))))
    }

    fn read(&self, path: &RelPath) -> FsResult<String> {
        fs::read_to_string(self.native(path)).map_err(|err| FsError::new("read", path, err))
    }

    fn write(&self, path: &RelPath, contents: &str) -> FsResult<()> {
        if let Some(parent) = path.parent() {
            self.create_dir_all(&parent)?;
        }
        fs::write(self.native(path), contents).map_err(|err| FsError::new("write", path, err))
    }

    fn create_dir_all(&self, path: &RelPath) -> FsResult<()> {
        fs::create_dir_all(self.native(path))
            .map_err(|err| FsError::new("create the directory", path, err))
    }

    fn link(
        &self,
        via: Via,
        node: NodeKind,
        canonical: &RelPath,
        target: &RelPath,
    ) -> FsResult<()> {
        if let Some(parent) = target.parent() {
            self.create_dir_all(&parent)?;
        }
        let link_path = self.native(target);

        match via {
            Via::Symlink => {
                // A relative target keeps the workspace movable: copy the
                // directory anywhere and the links still resolve.
                let relative = canonical.relative_to_dir(target.parent().as_ref());
                create_symlink(&relative, &link_path, node)
                    .map_err(|err| FsError::new("create a symlink at", target, err))
            }
            Via::Junction => {
                // Junctions cannot store a relative target, so this is absolute
                // by necessity rather than by choice.
                create_junction(&self.native(canonical), &link_path)
                    .map_err(|err| FsError::new("create a junction at", target, err))
            }
            Via::Import => Err(FsError::new(
                "create a link at",
                target,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "`import` writes a file and must not be routed through link()",
                ),
            )),
        }
    }

    fn remove_link(&self, path: &RelPath, node: NodeKind) -> FsResult<()> {
        let native = self.native(path);
        let meta = fs::symlink_metadata(&native)
            .map_err(|err| FsError::new("inspect the link at", path, err))?;

        // The guard that makes this tool safe to run unattended: if the entry is
        // not a link, it holds real content, and removing it would destroy work.
        if !meta.file_type().is_symlink() {
            return Err(FsError::new(
                "remove the link at",
                path,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "refusing to remove: this path holds real content, not a link",
                ),
            ));
        }

        remove_link_native(&native, node, &meta)
            .map_err(|err| FsError::new("remove the link at", path, err))
    }

    fn remove_file(&self, path: &RelPath) -> FsResult<()> {
        fs::remove_file(self.native(path)).map_err(|err| FsError::new("remove", path, err))
    }

    fn remove_empty_dir(&self, path: &RelPath) -> FsResult<()> {
        fs::remove_dir(self.native(path))
            .map_err(|err| FsError::new("remove the directory", path, err))
    }

    fn rename(&self, from: &RelPath, to: &RelPath) -> FsResult<()> {
        if let Some(parent) = to.parent() {
            self.create_dir_all(&parent)?;
        }
        fs::rename(self.native(from), self.native(to))
            .map_err(|err| FsError::new("move", from, err))
    }

    fn is_empty_dir(&self, path: &RelPath) -> FsResult<bool> {
        let mut entries =
            fs::read_dir(self.native(path)).map_err(|err| FsError::new("list", path, err))?;
        Ok(entries.next().is_none())
    }

    fn support(&self) -> LinkSupport {
        self.support
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Removes a Windows `\\?\` verbatim prefix so paths compare as users write them.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

/// Resolves `.` and `..` lexically, without touching the filesystem.
///
/// Lexical resolution is the right choice here: it lets a link target be
/// interpreted even when it dangles, which is exactly when the user most needs a
/// clear diagnostic.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Platform primitives
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn link_node_kind(_native: &Path, meta: &fs::Metadata) -> NodeKind {
    use std::os::windows::fs::FileTypeExt;
    if meta.file_type().is_symlink_dir() {
        NodeKind::Dir
    } else {
        NodeKind::File
    }
}

#[cfg(not(windows))]
fn link_node_kind(native: &Path, _meta: &fs::Metadata) -> NodeKind {
    // Following the link is the only way to tell on Unix. A dangling link has no
    // answer; `File` is a safe default because the domain never uses the node
    // kind of a link entry to decide what to remove.
    match fs::metadata(native) {
        Ok(meta) if meta.is_dir() => NodeKind::Dir,
        _ => NodeKind::File,
    }
}

#[cfg(windows)]
fn create_symlink(relative_target: &str, link: &Path, node: NodeKind) -> io::Result<()> {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    // Reparse points store the target verbatim, so it must use native separators.
    let target = PathBuf::from(relative_target.replace('/', "\\"));
    match node {
        NodeKind::Dir => symlink_dir(target, link),
        NodeKind::File => symlink_file(target, link),
    }
}

#[cfg(not(windows))]
fn create_symlink(relative_target: &str, link: &Path, _node: NodeKind) -> io::Result<()> {
    std::os::unix::fs::symlink(relative_target, link)
}

#[cfg(windows)]
fn create_junction(absolute_target: &Path, link: &Path) -> io::Result<()> {
    junction::create(absolute_target, link)
}

#[cfg(not(windows))]
fn create_junction(_absolute_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "junctions exist only on Windows",
    ))
}

#[cfg(windows)]
fn remove_link_native(native: &Path, _node: NodeKind, meta: &fs::Metadata) -> io::Result<()> {
    use std::os::windows::fs::FileTypeExt;
    // Directory links — symlinks and junctions alike — are removed with
    // `remove_dir`, which unlinks the reparse point and never touches the
    // content it points at.
    if meta.file_type().is_symlink_dir() {
        fs::remove_dir(native)
    } else {
        fs::remove_file(native)
    }
}

#[cfg(not(windows))]
fn remove_link_native(native: &Path, _node: NodeKind, _meta: &fs::Metadata) -> io::Result<()> {
    fs::remove_file(native)
}

/// Determines which link primitives this host currently allows.
#[cfg(windows)]
fn probe_support() -> LinkSupport {
    LinkSupport {
        symlink_file: can_symlink(),
        symlink_dir: can_symlink(),
        junction: true,
    }
}

#[cfg(not(windows))]
fn probe_support() -> LinkSupport {
    LinkSupport::FULL
}

/// Attempts a throwaway symlink to see whether this process holds
/// `SeCreateSymbolicLinkPrivilege`.
///
/// Windows grants it only to elevated processes or when Developer Mode is on, so
/// it cannot be inferred from the platform alone — it has to be tried.
#[cfg(windows)]
fn can_symlink() -> bool {
    use std::os::windows::fs::symlink_file;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        .wrapping_add(u128::from(std::process::id()));
    let probe = std::env::temp_dir().join(format!("agentlink-symlink-probe-{nonce}"));

    // Windows permits dangling symlinks, so no real target is needed.
    let allowed = symlink_file("agentlink-probe-target", &probe).is_ok();
    let _ = fs::remove_file(&probe);
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_resolves_parent_segments_lexically() {
        assert_eq!(normalise(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalise(Path::new("./a/./b")), PathBuf::from("a/b"));
    }

    #[test]
    fn strip_verbatim_removes_the_windows_prefix() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\repo")),
            PathBuf::from(r"C:\repo")
        );
        assert_eq!(
            strip_verbatim(Path::new("/home/repo")),
            PathBuf::from("/home/repo")
        );
    }
}
