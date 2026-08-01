//! Integration tests against a real filesystem.
//!
//! Unit tests cover the decision logic against a simulated host. These verify
//! the part that cannot be simulated: that the links agentlink creates behave the
//! way the whole design depends on them behaving.
//!
//! The central claim under test is that propagation is a property of the
//! filesystem rather than a feature anyone implemented — so writing, renaming and
//! deleting through one agent's path is immediately visible through every other.

use std::fs;
use std::path::Path;

use agentlink_core::model::{Entry, LinkSupport, LinkTarget, NodeKind, Via};
use agentlink_core::path::RelPath;
use agentlink_core::workspace::Workspace;
use agentlink_fs::RootedWorkspace;

fn rel(path: &str) -> RelPath {
    RelPath::new(path).expect("valid test path")
}

/// A workspace in a temp directory, kept alive by the returned guard.
fn workspace() -> (tempfile::TempDir, RootedWorkspace) {
    let temp = tempfile::tempdir().expect("temp dir");
    let ws = RootedWorkspace::open(temp.path()).expect("open workspace");
    (temp, ws)
}

/// The mechanism this host can use to link a directory, if any.
fn dir_via(support: LinkSupport) -> Option<Via> {
    support.best_for(NodeKind::Dir)
}

#[test]
fn probing_distinguishes_files_directories_and_absence() {
    let (temp, ws) = workspace();
    fs::write(temp.path().join("AGENTS.md"), "# rules").unwrap();
    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();

    assert_eq!(ws.probe(&rel("AGENTS.md")).unwrap(), Some(Entry::file()));
    assert_eq!(
        ws.probe(&rel(".agents/skills")).unwrap(),
        Some(Entry::dir())
    );
    assert_eq!(ws.probe(&rel("nope.md")).unwrap(), None);
}

#[test]
fn a_linked_directory_is_the_same_directory() {
    // The property the entire project rests on. If this fails, nothing else in
    // agentlink is true.
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();

    let Some(via) = dir_via(ws.support()) else {
        eprintln!("skipped: this host cannot link directories at all");
        return;
    };

    ws.link(
        via,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();
    ws.link(
        via,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".cursor/skills"),
    )
    .unwrap();

    // Written through Cursor's path...
    fs::create_dir_all(temp.path().join(".cursor/skills/deploy")).unwrap();
    fs::write(
        temp.path().join(".cursor/skills/deploy/SKILL.md"),
        "---\nname: deploy\n---\n",
    )
    .unwrap();

    // ...visible through Claude Code's path and the canonical one, with no
    // synchronisation step in between.
    assert_eq!(
        fs::read_to_string(temp.path().join(".claude/skills/deploy/SKILL.md")).unwrap(),
        "---\nname: deploy\n---\n"
    );
    assert!(temp.path().join(".agents/skills/deploy/SKILL.md").exists());

    // A rename through one path is a rename everywhere.
    fs::rename(
        temp.path().join(".claude/skills/deploy"),
        temp.path().join(".claude/skills/release"),
    )
    .unwrap();
    assert!(temp.path().join(".cursor/skills/release").exists());
    assert!(!temp.path().join(".agents/skills/deploy").exists());

    // So is a deletion.
    fs::remove_dir_all(temp.path().join(".cursor/skills/release")).unwrap();
    assert!(!temp.path().join(".agents/skills/release").exists());
    assert!(!temp.path().join(".claude/skills/release").exists());
}

#[test]
fn a_created_link_is_probed_back_as_pointing_at_its_canonical_target() {
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();

    let Some(via) = dir_via(ws.support()) else {
        eprintln!("skipped: this host cannot link directories at all");
        return;
    };
    ws.link(
        via,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();

    // Round-tripping through the filesystem must yield a workspace-relative
    // target, otherwise the planner cannot recognise its own work and would
    // re-create the link on every run.
    let entry = ws
        .probe(&rel(".claude/skills"))
        .unwrap()
        .expect("link exists");
    assert_eq!(entry.link, Some(LinkTarget::Inside(rel(".agents/skills"))));
}

#[test]
fn removing_a_link_leaves_the_content_it_pointed_at() {
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".agents/skills/review")).unwrap();
    fs::write(temp.path().join(".agents/skills/review/SKILL.md"), "body").unwrap();

    let Some(via) = dir_via(ws.support()) else {
        eprintln!("skipped: this host cannot link directories at all");
        return;
    };
    ws.link(
        via,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();
    ws.remove_link(&rel(".claude/skills"), NodeKind::Dir)
        .unwrap();

    assert!(!temp.path().join(".claude/skills").exists());
    // The canonical content survives — we unlinked, we did not delete.
    assert_eq!(
        fs::read_to_string(temp.path().join(".agents/skills/review/SKILL.md")).unwrap(),
        "body"
    );
}

#[test]
fn removing_a_link_refuses_when_the_path_holds_real_content() {
    // The guard that makes `agentlink clean` safe to run unattended.
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".claude/skills/mine")).unwrap();
    fs::write(temp.path().join(".claude/skills/mine/SKILL.md"), "precious").unwrap();

    let err = ws
        .remove_link(&rel(".claude/skills"), NodeKind::Dir)
        .unwrap_err();
    assert!(err.to_string().contains(".claude/skills"));
    assert!(temp.path().join(".claude/skills/mine/SKILL.md").exists());
}

#[test]
fn symlinks_store_a_relative_target_so_the_workspace_can_move() {
    let (temp, ws) = workspace();
    if !ws.support().symlink_dir {
        eprintln!("skipped: symlinks unavailable (Windows without Developer Mode or elevation)");
        return;
    }
    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();
    ws.link(
        Via::Symlink,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();

    let stored = fs::read_link(temp.path().join(".claude/skills")).unwrap();
    assert!(
        stored.is_relative(),
        "symlink stored an absolute target ({stored:?}), which breaks when the workspace moves"
    );
    // A relative link can never go stale from a move, so `doctor` stays quiet.
    assert!(ws.stale_junctions(&[rel(".claude/skills")]).is_empty());
}

#[cfg(windows)]
#[test]
fn junctions_work_without_elevation() {
    // The fact that makes agentlink viable on Windows out of the box: creating a
    // directory junction has never required a privilege.
    let (temp, ws) = workspace();
    assert!(
        ws.support().junction,
        "junctions should always be available on Windows"
    );

    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();
    ws.link(
        Via::Junction,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();

    fs::write(temp.path().join(".agents/skills/note.md"), "hello").unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join(".claude/skills/note.md")).unwrap(),
        "hello"
    );
}

#[cfg(windows)]
#[test]
fn a_junction_left_behind_by_a_move_is_reported_as_stale() {
    // Junctions record an absolute path, so a copied or moved workspace keeps
    // resolving to the original. Detecting that is `doctor`'s job.
    let source = tempfile::tempdir().unwrap();
    let ws = RootedWorkspace::open(source.path()).unwrap();
    fs::create_dir_all(source.path().join(".agents/skills")).unwrap();
    ws.link(
        Via::Junction,
        NodeKind::Dir,
        &rel(".agents/skills"),
        &rel(".claude/skills"),
    )
    .unwrap();

    // Look at that same junction from a workspace rooted elsewhere.
    let elsewhere = tempfile::tempdir().unwrap();
    copy_tree(source.path(), elsewhere.path());
    let moved = RootedWorkspace::open(elsewhere.path()).unwrap();

    let stale = moved.stale_junctions(&[rel(".claude/skills")]);
    assert_eq!(
        stale.len(),
        1,
        "a junction pointing at the old root should be flagged"
    );
    assert_eq!(stale[0].0, rel(".claude/skills"));
}

#[test]
fn writes_create_missing_parent_directories() {
    let (temp, ws) = workspace();
    ws.write(&rel(".agentlink/config.toml"), "version = 1\n")
        .unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join(".agentlink/config.toml")).unwrap(),
        "version = 1\n"
    );
}

#[test]
fn renaming_creates_the_destination_parent() {
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".claude/skills/review")).unwrap();
    fs::write(temp.path().join(".claude/skills/review/SKILL.md"), "body").unwrap();

    // This is what adoption does: `.claude/skills` moves to `.agents/skills`
    // while `.agents` does not exist yet.
    ws.rename(&rel(".claude/skills"), &rel(".agents/skills"))
        .unwrap();

    assert_eq!(
        fs::read_to_string(temp.path().join(".agents/skills/review/SKILL.md")).unwrap(),
        "body"
    );
    assert!(!temp.path().join(".claude/skills").exists());
}

#[test]
fn empty_directory_detection_is_accurate() {
    let (temp, ws) = workspace();
    fs::create_dir_all(temp.path().join(".agents/skills")).unwrap();
    assert!(ws.is_empty_dir(&rel(".agents/skills")).unwrap());

    fs::write(temp.path().join(".agents/skills/note.md"), "x").unwrap();
    assert!(!ws.is_empty_dir(&rel(".agents/skills")).unwrap());
}

#[test]
fn the_workspace_root_is_free_of_windows_verbatim_prefixes() {
    // `canonicalize` yields `\\?\C:\...` on Windows, which would leak into every
    // message agentlink prints.
    let (_temp, ws) = workspace();
    assert!(
        !ws.root().to_string_lossy().starts_with(r"\\?\"),
        "root should be display-friendly, got {:?}",
        ws.root()
    );
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        if meta.file_type().is_symlink() {
            // Reproduce the reparse point verbatim, which is exactly how a naive
            // copy leaves a junction pointing at the original location.
            #[cfg(windows)]
            {
                let raw = fs::read_link(entry.path()).unwrap();
                let _ = junction_create(&raw, &target);
            }
        } else if meta.is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[cfg(windows)]
fn junction_create(target: &Path, link: &Path) -> std::io::Result<()> {
    junction::create(target, link)
}
