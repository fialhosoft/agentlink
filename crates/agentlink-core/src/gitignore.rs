//! Maintaining a managed block in `.gitignore`.
//!
//! Because the recommended posture is "commit the canonical layout, materialise
//! everything else locally", the provider paths must be ignored. Editing a file
//! the user also owns calls for care, so this is a pure string transformation
//! with exhaustive tests rather than an ad-hoc append:
//!
//! * lines outside the markers are never touched, in content or in order;
//! * the block is rewritten in place, so entries that stop applying disappear;
//! * removing the block leaves the rest of the file byte-identical.

const BEGIN: &str = "# >>> agentlink >>>";
const END: &str = "# <<< agentlink <<<";

const PREAMBLE: &str = "\
# Materialised by `agentlink apply` from the canonical layout (AGENTS.md,
# .agents/). Commit the canonical layout; these paths are per-developer.
# Managed automatically — edits inside this block are overwritten.";

/// Rewrites the managed block, returning the new file contents.
///
/// Passing an empty `entries` removes the block entirely. The file's dominant
/// line ending is preserved: silently converting a CRLF `.gitignore` to LF would
/// show up as a whole-file diff for every Windows contributor.
pub fn update(existing: &str, entries: &[String]) -> String {
    // Nothing to add and nothing of ours to remove: leave the file byte-identical.
    if entries.is_empty() && !is_managed(existing) {
        return existing.to_string();
    }

    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (before, after) = split(existing);

    if entries.is_empty() {
        return join(&before, "", &after, newline);
    }

    let mut block = String::with_capacity(256);
    block.push_str(BEGIN);
    block.push_str(newline);
    for line in PREAMBLE.lines() {
        block.push_str(line);
        block.push_str(newline);
    }
    for entry in entries {
        // A leading slash anchors the pattern to the repository root, so a
        // provider path like `CLAUDE.md` cannot accidentally ignore a file of the
        // same name nested somewhere else.
        block.push('/');
        block.push_str(entry.trim_start_matches('/'));
        block.push_str(newline);
    }
    block.push_str(END);

    join(&before, &block, &after, newline)
}

/// Whether the file already contains a managed block.
pub fn is_managed(existing: &str) -> bool {
    existing.lines().any(|line| line.trim_end() == BEGIN)
}

/// Splits a file into the parts before and after the managed block.
fn split(existing: &str) -> (Vec<&str>, Vec<&str>) {
    let lines: Vec<&str> = existing.lines().collect();
    let begin = lines.iter().position(|line| line.trim_end() == BEGIN);
    let Some(begin) = begin else {
        return (lines, Vec::new());
    };
    // An unterminated block (hand-edited, or a truncated write) is treated as
    // running to the end of the file rather than silently duplicating markers.
    let end = lines[begin..]
        .iter()
        .position(|line| line.trim_end() == END)
        .map_or(lines.len(), |offset| begin + offset + 1);

    (lines[..begin].to_vec(), lines[end..].to_vec())
}

fn join(before: &[&str], block: &str, after: &[&str], newline: &str) -> String {
    let mut sections: Vec<String> = Vec::new();

    let before = trim_trailing_blanks(before);
    if !before.is_empty() {
        sections.push(before.join(newline));
    }
    if !block.is_empty() {
        sections.push(block.to_string());
    }
    let after = trim_leading_blanks(after);
    if !after.is_empty() {
        sections.push(after.join(newline));
    }

    if sections.is_empty() {
        return String::new();
    }
    let separator = format!("{newline}{newline}");
    format!("{}{newline}", sections.join(&separator))
}

fn trim_trailing_blanks<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(0, |i| i + 1);
    lines[..end].to_vec()
}

fn trim_leading_blanks<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    lines[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(paths: &[&str]) -> Vec<String> {
        paths.iter().copied().map(String::from).collect()
    }

    #[test]
    fn adds_a_block_to_an_empty_file() {
        let result = update("", &entries(["CLAUDE.md", ".claude/skills"].as_slice()));
        assert!(result.starts_with(BEGIN));
        assert!(result.contains("\n/CLAUDE.md\n"));
        assert!(result.contains("\n/.claude/skills\n"));
        assert!(result.trim_end().ends_with(END));
    }

    #[test]
    fn preserves_existing_content_around_the_block() {
        let original = "node_modules/\ntarget/\n";
        let result = update(original, &entries(["CLAUDE.md"].as_slice()));
        assert!(result.starts_with("node_modules/\ntarget/\n"));
        assert!(result.contains("/CLAUDE.md"));
    }

    #[test]
    fn rewriting_is_idempotent() {
        let paths = entries(["CLAUDE.md", ".claude/skills"].as_slice());
        let once = update("node_modules/\n", &paths);
        let twice = update(&once, &paths);
        assert_eq!(once, twice);
    }

    #[test]
    fn entries_that_stop_applying_are_removed() {
        let first = update("", &entries(["CLAUDE.md", ".cursor/skills"].as_slice()));
        let second = update(&first, &entries(["CLAUDE.md"].as_slice()));
        assert!(second.contains("/CLAUDE.md"));
        assert!(!second.contains(".cursor/skills"));
        assert_eq!(second.matches(BEGIN).count(), 1);
    }

    #[test]
    fn removing_the_block_restores_the_surrounding_file() {
        let original = "node_modules/\ntarget/\n";
        let with_block = update(original, &entries(["CLAUDE.md"].as_slice()));
        assert_eq!(update(&with_block, &[]), original);
    }

    #[test]
    fn user_content_after_the_block_survives_a_rewrite() {
        let original = update("before/\n", &entries(["CLAUDE.md"].as_slice()));
        let edited = format!("{original}\nafter/\n");
        let result = update(&edited, &entries([".claude/skills"].as_slice()));

        assert!(result.starts_with("before/"));
        assert!(result.trim_end().ends_with("after/"));
        assert!(result.contains("/.claude/skills"));
        assert!(!result.contains("/CLAUDE.md"));
        assert_eq!(result.matches(BEGIN).count(), 1);
    }

    #[test]
    fn an_unterminated_block_is_repaired_rather_than_duplicated() {
        let broken = format!("keep/\n{BEGIN}\n/stale\n");
        let result = update(&broken, &entries(["CLAUDE.md"].as_slice()));
        assert_eq!(result.matches(BEGIN).count(), 1);
        assert_eq!(result.matches(END).count(), 1);
        assert!(!result.contains("/stale"));
        assert!(result.starts_with("keep/"));
    }

    #[test]
    fn paths_are_anchored_to_the_repository_root() {
        // Without the leading slash, `CLAUDE.md` would also ignore
        // `docs/vendor/CLAUDE.md`, which agentlink does not manage.
        let result = update("", &entries(["CLAUDE.md"].as_slice()));
        assert!(result.contains("\n/CLAUDE.md\n"));
    }

    #[test]
    fn a_crlf_file_stays_crlf() {
        // Rewriting a Windows .gitignore to LF would show up as a whole-file diff
        // for every contributor on that platform.
        let original = "node_modules/\r\ntarget/\r\n";
        let result = update(original, &entries(["CLAUDE.md"].as_slice()));

        assert!(result.contains("\r\n"), "line endings were converted to LF");
        assert!(
            !result.replace("\r\n", "").contains('\n'),
            "output mixes CRLF and LF"
        );
        assert!(result.contains("/CLAUDE.md\r\n"));
    }

    #[test]
    fn an_lf_file_stays_lf() {
        let result = update("node_modules/\n", &entries(["CLAUDE.md"].as_slice()));
        assert!(!result.contains('\r'));
    }

    #[test]
    fn a_file_with_no_block_and_nothing_to_add_is_left_byte_identical() {
        // `apply` calls this on every run. Touching an unrelated .gitignore — even
        // just to normalise whitespace — would put noise in someone's diff.
        for original in ["node_modules/\r\ntarget/\r\n", "a/\n\n\n", ""] {
            assert_eq!(update(original, &[]), original);
        }
    }

    #[test]
    fn detects_whether_a_file_is_already_managed() {
        assert!(!is_managed("node_modules/\n"));
        assert!(is_managed(&update("", &entries(["CLAUDE.md"].as_slice()))));
    }

    #[test]
    fn output_always_ends_with_exactly_one_newline() {
        for input in ["", "a/\n", "a/\n\n\n"] {
            let result = update(input, &entries(["CLAUDE.md"].as_slice()));
            assert!(result.ends_with('\n'));
            assert!(!result.ends_with("\n\n"));
        }
    }
}
