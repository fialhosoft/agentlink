//! An in-memory [`Workspace`] for tests.
//!
//! Planning decisions depend on which link primitives a host allows, and that
//! varies *within* an operating system — Windows grants symlink creation only
//! under Developer Mode or elevation. Simulating those hosts in memory means the
//! full decision matrix is covered by fast, deterministic tests that run
//! identically on every CI runner, with real-filesystem tests reserved for
//! verifying the adapter itself.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;

use crate::model::{Entry, LinkSupport, LinkTarget, NodeKind, Via};
use crate::path::RelPath;
use crate::workspace::{FsError, FsResult, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    File(String),
    Dir,
    Link { node: NodeKind, target: String },
}

/// A simulated workspace with configurable link support.
#[derive(Debug)]
pub struct FakeWorkspace {
    nodes: RefCell<BTreeMap<String, Node>>,
    support: LinkSupport,
}

impl FakeWorkspace {
    /// A host where every link primitive works, as on Linux or macOS.
    pub fn unix() -> Self {
        Self::with_support(LinkSupport::FULL)
    }

    /// Windows without Developer Mode or elevation: junctions only.
    pub fn windows_unprivileged() -> Self {
        Self::with_support(LinkSupport::JUNCTION_ONLY)
    }

    /// Windows with Developer Mode enabled: symlinks and junctions.
    pub fn windows_developer_mode() -> Self {
        Self::with_support(LinkSupport {
            symlink_file: true,
            symlink_dir: true,
            junction: true,
        })
    }

    /// A host where nothing can be linked, used to prove the fallback path.
    pub fn without_links() -> Self {
        Self::with_support(LinkSupport::NONE)
    }

    pub fn with_support(support: LinkSupport) -> Self {
        Self {
            nodes: RefCell::new(BTreeMap::new()),
            support,
        }
    }

    pub fn add_file(&self, path: &str, contents: &str) {
        let path = RelPath::new(path).expect("valid test path");
        self.ensure_parents(&path);
        self.nodes
            .borrow_mut()
            .insert(path.to_string(), Node::File(contents.to_string()));
    }

    pub fn add_dir(&self, path: &str) {
        let path = RelPath::new(path).expect("valid test path");
        self.ensure_parents(&path);
        self.nodes.borrow_mut().insert(path.to_string(), Node::Dir);
    }

    pub fn add_link(&self, path: &str, node: NodeKind, target: &str) {
        let path = RelPath::new(path).expect("valid test path");
        let target = RelPath::new(target).expect("valid test path");
        self.ensure_parents(&path);
        self.nodes.borrow_mut().insert(
            path.to_string(),
            Node::Link {
                node,
                target: target.to_string(),
            },
        );
    }

    /// Every path currently present, for assertions.
    pub fn paths(&self) -> Vec<String> {
        self.nodes.borrow().keys().cloned().collect()
    }

    /// The contents of a file, without following links.
    pub fn raw_file(&self, path: &str) -> Option<String> {
        match self.nodes.borrow().get(path) {
            Some(Node::File(contents)) => Some(contents.clone()),
            _ => None,
        }
    }

    /// The immediate target of a link, if `path` is one.
    pub fn link_target(&self, path: &str) -> Option<String> {
        match self.nodes.borrow().get(path) {
            Some(Node::Link { target, .. }) => Some(target.clone()),
            _ => None,
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        self.nodes.borrow().contains_key(path)
    }

    fn ensure_parents(&self, path: &RelPath) {
        let mut current = path.parent();
        let mut nodes = self.nodes.borrow_mut();
        while let Some(dir) = current {
            nodes.entry(dir.to_string()).or_insert(Node::Dir);
            current = dir.parent();
        }
    }

    /// Follows links to the concrete path they ultimately name.
    fn resolve(&self, path: &RelPath) -> Option<String> {
        let nodes = self.nodes.borrow();
        let mut current = path.to_string();
        for _ in 0..16 {
            match nodes.get(&current) {
                Some(Node::Link { target, .. }) => current.clone_from(target),
                Some(_) => return Some(current),
                None => return None,
            }
        }
        None
    }

    fn missing(operation: &'static str, path: &RelPath) -> FsError {
        FsError::new(
            operation,
            path,
            io::Error::new(
                io::ErrorKind::NotFound,
                "no such entry in the fake workspace",
            ),
        )
    }
}

impl Workspace for FakeWorkspace {
    fn probe(&self, path: &RelPath) -> FsResult<Option<Entry>> {
        Ok(self
            .nodes
            .borrow()
            .get(path.as_str())
            .map(|node| match node {
                Node::File(_) => Entry::file(),
                Node::Dir => Entry::dir(),
                Node::Link { node, target } => Entry::link(
                    *node,
                    match RelPath::new(target) {
                        Ok(rel) => LinkTarget::Inside(rel),
                        Err(_) => LinkTarget::Outside(target.clone()),
                    },
                ),
            }))
    }

    fn read(&self, path: &RelPath) -> FsResult<String> {
        let resolved = self
            .resolve(path)
            .ok_or_else(|| Self::missing("read", path))?;
        match self.nodes.borrow().get(&resolved) {
            Some(Node::File(contents)) => Ok(contents.clone()),
            _ => Err(Self::missing("read", path)),
        }
    }

    fn write(&self, path: &RelPath, contents: &str) -> FsResult<()> {
        self.ensure_parents(path);
        self.nodes
            .borrow_mut()
            .insert(path.to_string(), Node::File(contents.to_string()));
        Ok(())
    }

    fn create_dir_all(&self, path: &RelPath) -> FsResult<()> {
        self.ensure_parents(path);
        self.nodes
            .borrow_mut()
            .entry(path.to_string())
            .or_insert(Node::Dir);
        Ok(())
    }

    fn link(
        &self,
        via: Via,
        node: NodeKind,
        canonical: &RelPath,
        target: &RelPath,
    ) -> FsResult<()> {
        debug_assert!(via.is_link(), "link() called with a non-link mechanism");
        self.ensure_parents(target);
        self.nodes.borrow_mut().insert(
            target.to_string(),
            Node::Link {
                node,
                target: canonical.to_string(),
            },
        );
        Ok(())
    }

    fn remove_link(&self, path: &RelPath, _node: NodeKind) -> FsResult<()> {
        let mut nodes = self.nodes.borrow_mut();
        match nodes.get(path.as_str()) {
            Some(Node::Link { .. }) => {
                nodes.remove(path.as_str());
                Ok(())
            }
            Some(_) => Err(FsError::new(
                "remove link at",
                path,
                io::Error::new(io::ErrorKind::InvalidInput, "entry is not a link"),
            )),
            None => Err(Self::missing("remove link at", path)),
        }
    }

    fn remove_file(&self, path: &RelPath) -> FsResult<()> {
        let mut nodes = self.nodes.borrow_mut();
        match nodes.get(path.as_str()) {
            Some(Node::File(_)) => {
                nodes.remove(path.as_str());
                Ok(())
            }
            Some(_) => Err(FsError::new(
                "remove file at",
                path,
                io::Error::new(io::ErrorKind::InvalidInput, "entry is not a regular file"),
            )),
            None => Err(Self::missing("remove file at", path)),
        }
    }

    fn remove_empty_dir(&self, path: &RelPath) -> FsResult<()> {
        if !self.is_empty_dir(path)? {
            return Err(FsError::new(
                "remove directory at",
                path,
                io::Error::new(io::ErrorKind::DirectoryNotEmpty, "directory is not empty"),
            ));
        }
        self.nodes.borrow_mut().remove(path.as_str());
        Ok(())
    }

    fn rename(&self, from: &RelPath, to: &RelPath) -> FsResult<()> {
        self.ensure_parents(to);
        let mut nodes = self.nodes.borrow_mut();
        if !nodes.contains_key(from.as_str()) {
            return Err(Self::missing("rename", from));
        }

        let prefix = format!("{from}/");
        let moving: Vec<String> = nodes
            .keys()
            .filter(|key| key.as_str() == from.as_str() || key.starts_with(&prefix))
            .cloned()
            .collect();

        for key in moving {
            let node = nodes.remove(&key).expect("key was just listed");
            let suffix = key.strip_prefix(from.as_str()).unwrap_or_default();
            nodes.insert(format!("{to}{suffix}"), node);
        }
        Ok(())
    }

    fn is_empty_dir(&self, path: &RelPath) -> FsResult<bool> {
        let nodes = self.nodes.borrow();
        match nodes.get(path.as_str()) {
            Some(Node::Dir) => {
                let prefix = format!("{path}/");
                Ok(!nodes.keys().any(|key| key.starts_with(&prefix)))
            }
            Some(_) => Ok(false),
            None => Err(Self::missing("inspect", path)),
        }
    }

    fn support(&self) -> LinkSupport {
        self.support
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    #[test]
    fn adding_a_nested_file_creates_its_parents() {
        let ws = FakeWorkspace::unix();
        ws.add_file(".agents/skills/review/SKILL.md", "body");
        assert!(ws.exists(".agents"));
        assert!(ws.exists(".agents/skills"));
        assert!(ws.exists(".agents/skills/review"));
    }

    #[test]
    fn reads_follow_links() {
        let ws = FakeWorkspace::unix();
        ws.add_file("AGENTS.md", "# rules");
        ws.add_link("CLAUDE.md", NodeKind::File, "AGENTS.md");
        assert_eq!(ws.read(&rel("CLAUDE.md")).unwrap(), "# rules");
    }

    #[test]
    fn empty_directory_detection_ignores_the_directory_itself() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".agents/skills");
        assert!(ws.is_empty_dir(&rel(".agents/skills")).unwrap());
        ws.add_file(".agents/skills/review/SKILL.md", "body");
        assert!(!ws.is_empty_dir(&rel(".agents/skills")).unwrap());
    }

    #[test]
    fn rename_moves_the_whole_subtree() {
        let ws = FakeWorkspace::unix();
        ws.add_file(".claude/skills/review/SKILL.md", "body");

        ws.rename(&rel(".claude/skills"), &rel(".agents/skills"))
            .unwrap();

        assert!(!ws.exists(".claude/skills"));
        assert!(ws.exists(".agents/skills"));
        assert_eq!(
            ws.raw_file(".agents/skills/review/SKILL.md").as_deref(),
            Some("body")
        );
    }

    #[test]
    fn removing_a_link_never_removes_real_content() {
        let ws = FakeWorkspace::unix();
        ws.add_dir(".claude/skills");
        assert!(
            ws.remove_link(&rel(".claude/skills"), NodeKind::Dir)
                .is_err()
        );
        assert!(ws.exists(".claude/skills"));
    }
}
