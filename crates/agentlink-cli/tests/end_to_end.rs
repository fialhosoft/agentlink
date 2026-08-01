//! End-to-end tests driving the real binary against real directories.
//!
//! These encode the promises the README makes, so a regression in any of them is
//! a failing build rather than a bug report.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Exit code meaning "work is pending or blocked".
const EXIT_PENDING: i32 = 2;

fn agentlink() -> PathBuf {
    // Cargo builds integration-test binaries next to the crate's own binaries.
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!("agentlink{}", std::env::consts::EXE_SUFFIX))
}

struct Run {
    stdout: String,
    code: i32,
}

fn run(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(agentlink())
        .arg("--color")
        .arg("never")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("agentlink should be built before integration tests run");

    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        code: output.status.code().unwrap_or(-1),
    }
}

/// A repository that has only ever been used with Claude Code.
fn claude_only_repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join(".claude/skills/code-review")).unwrap();
    fs::write(
        root.join("CLAUDE.md"),
        "# My project\n\nUse pnpm, never npm.\n",
    )
    .unwrap();
    fs::write(
        root.join(".claude/skills/code-review/SKILL.md"),
        "---\nname: code-review\ndescription: Reviews a diff\n---\n\nBe strict.\n",
    )
    .unwrap();
    temp
}

#[test]
fn init_never_seeds_over_content_waiting_to_be_adopted() {
    // Writing a template AGENTS.md here would turn a clean one-sided adoption
    // into a two-sided merge that only a human could resolve.
    let repo = claude_only_repo();
    let result = run(repo.path(), &["init"]);

    assert_eq!(
        result.code, EXIT_PENDING,
        "init should report blocked work:\n{}",
        result.stdout
    );
    assert!(
        !repo.path().join("AGENTS.md").exists(),
        "AGENTS.md must not be seeded yet"
    );
    assert!(
        result.stdout.contains("agentlink adopt"),
        "should name the fix:\n{}",
        result.stdout
    );
    // The user's file is untouched.
    assert!(
        fs::read_to_string(repo.path().join("CLAUDE.md"))
            .unwrap()
            .contains("Use pnpm")
    );
}

#[test]
fn adopt_moves_content_canonically_and_serves_every_agent_in_one_pass() {
    let repo = claude_only_repo();
    let root = repo.path();

    run(root, &["init"]);
    let result = run(root, &["adopt"]);
    assert_eq!(result.code, 0, "adopt should succeed:\n{}", result.stdout);

    // Content moved to the canonical location, unchanged.
    assert!(
        fs::read_to_string(root.join("AGENTS.md"))
            .unwrap()
            .contains("Use pnpm")
    );
    assert!(root.join(".agents/skills/code-review/SKILL.md").exists());

    // Claude Code still reads its own paths.
    assert!(root.join(".claude/skills/code-review/SKILL.md").exists());
    assert!(root.join("CLAUDE.md").exists());

    // Adoption creates the canonical resource, which must unblock every provider
    // that was waiting for it — within the same command.
    for path in [".cursor/skills", ".github/skills", ".opencode/skills"] {
        assert!(
            root.join(path).join("code-review/SKILL.md").exists(),
            "{path} should have been linked in the same run:\n{}",
            result.stdout
        );
    }
    assert!(result.stdout.contains("0 files copied"));
}

#[test]
fn apply_is_idempotent() {
    // The precondition for wiring this into a git hook.
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);
    run(root, &["adopt"]);

    let second = run(root, &["apply"]);
    assert_eq!(second.code, 0);
    assert!(
        second.stdout.contains("already up to date"),
        "a second apply should change nothing:\n{}",
        second.stdout
    );

    let third = run(root, &["status", "--check"]);
    assert_eq!(
        third.code, 0,
        "status --check should be clean:\n{}",
        third.stdout
    );
}

#[test]
fn a_hand_written_file_is_never_replaced() {
    // Both sides hold content, so agentlink must refuse and say so.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("AGENTS.md"), "# shared rules\n").unwrap();
    fs::write(root.join("CLAUDE.md"), "# hand written, do not touch\n").unwrap();

    run(root, &["init"]);
    let result = run(root, &["apply"]);

    assert_eq!(result.code, EXIT_PENDING);
    assert_eq!(
        fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
        "# hand written, do not touch\n",
        "agentlink overwrote a file it did not create"
    );
    assert!(
        result.stdout.contains("merge them by hand"),
        "{}",
        result.stdout
    );
}

#[test]
fn status_check_signals_pending_work_for_ci() {
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);

    let pending = run(root, &["status", "--check"]);
    assert_eq!(pending.code, EXIT_PENDING);

    run(root, &["adopt"]);
    let clean = run(root, &["status", "--check"]);
    assert_eq!(clean.code, 0, "{}", clean.stdout);
}

#[test]
fn clean_removes_only_what_agentlink_created() {
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);
    run(root, &["adopt"]);

    let result = run(root, &["clean"]);
    assert_eq!(result.code, 0, "{}", result.stdout);

    // Every materialised path is gone...
    assert!(!root.join(".cursor/skills").exists());
    assert!(!root.join("CLAUDE.md").exists());
    // ...and the canonical layout, which holds the actual content, is intact.
    assert!(
        fs::read_to_string(root.join("AGENTS.md"))
            .unwrap()
            .contains("Use pnpm")
    );
    assert!(root.join(".agents/skills/code-review/SKILL.md").exists());
}

#[test]
fn dry_run_writes_nothing() {
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);

    let result = run(root, &["adopt", "--dry-run"]);
    assert!(result.stdout.contains("dry run"), "{}", result.stdout);
    assert!(!root.join("AGENTS.md").exists(), "dry run moved content");
    assert!(root.join(".claude/skills/code-review/SKILL.md").exists());
}

#[test]
fn commands_refuse_to_run_before_init() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(temp.path(), &["apply"]);
    assert_eq!(result.code, 1);
}

#[test]
fn providers_lists_the_capability_matrix() {
    let temp = tempfile::tempdir().unwrap();
    let result = run(temp.path(), &["providers"]);

    assert_eq!(result.code, 0);
    for provider in [
        "claude-code",
        "antigravity",
        "codex",
        "cursor",
        "github-copilot",
        "opencode",
    ] {
        assert!(
            result.stdout.contains(provider),
            "missing {provider}:\n{}",
            result.stdout
        );
    }
    assert!(result.stdout.contains("native"));
}

#[test]
fn doctor_reports_a_healthy_workspace() {
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);
    run(root, &["adopt"]);

    let result = run(root, &["doctor"]);
    assert_eq!(result.code, 0, "doctor found problems:\n{}", result.stdout);
    assert!(
        result.stdout.contains("no problems found"),
        "{}",
        result.stdout
    );
}

#[test]
fn a_workspace_local_manifest_adds_an_agent_without_a_release() {
    // The escape hatch that keeps users unblocked when an agent changes its paths
    // upstream, and the same file they would send as a pull request.
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);
    run(root, &["adopt"]);

    fs::create_dir_all(root.join(".agentlink/providers")).unwrap();
    fs::write(
        root.join(".agentlink/providers/futureagent.toml"),
        "schema = 1\n\
         id = \"futureagent\"\n\
         name = \"Future Agent\"\n\n\
         [[capability]]\n\
         resource = \"skills\"\n\
         strategy = \"link\"\n\
         path = \".future/skills\"\n",
    )
    .unwrap();

    let result = run(root, &["apply"]);
    assert_eq!(result.code, 0, "{}", result.stdout);
    assert!(
        root.join(".future/skills/code-review/SKILL.md").exists(),
        "a local manifest should be served like any built-in:\n{}",
        result.stdout
    );
}

#[test]
fn an_invalid_local_manifest_fails_loudly_rather_than_being_ignored() {
    let repo = claude_only_repo();
    let root = repo.path();
    run(root, &["init"]);

    fs::create_dir_all(root.join(".agentlink/providers")).unwrap();
    fs::write(
        root.join(".agentlink/providers/broken.toml"),
        // `pathh` is a typo; silently ignoring it would leave the user believing
        // their agent is covered when nothing is.
        "schema = 1\nid = \"broken\"\nname = \"Broken\"\n\n\
         [[capability]]\nresource = \"skills\"\nstrategy = \"link\"\npathh = \".x/skills\"\n",
    )
    .unwrap();

    let result = run(root, &["status"]);
    assert_eq!(result.code, 1, "{}", result.stdout);
}

#[test]
fn the_gitignore_block_covers_every_materialised_path() {
    let repo = claude_only_repo();
    let root = repo.path();
    fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();

    run(root, &["init"]);
    run(root, &["adopt"]);

    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(
        gitignore.starts_with("node_modules/"),
        "existing entries must survive"
    );
    for path in [
        "/CLAUDE.md",
        "/.claude/skills",
        "/.cursor/skills",
        "/.github/skills",
    ] {
        assert!(gitignore.contains(path), "missing {path} in:\n{gitignore}");
    }
    // The canonical layout is what gets committed, so it must never be ignored.
    assert!(!gitignore.contains("/AGENTS.md"));
    assert!(!gitignore.contains("/.agents/skills"));

    // The lock is per-machine state — it records which primitive this host used,
    // which legitimately differs across a team — so it must be ignored.
    assert!(gitignore.contains("/.agentlink/lock.toml"), "{gitignore}");
    // The config is the shared decision and must stay tracked.
    assert!(!gitignore.contains("/.agentlink/config.toml"));
}
