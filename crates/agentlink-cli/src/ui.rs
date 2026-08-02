//! Terminal presentation.
//!
//! Colour is opt-out via `NO_COLOR` (the community convention) and is disabled
//! automatically when output is not a terminal, so piping `agentlink status`
//! into a file or a CI log yields clean text.

use std::io::IsTerminal;

/// Whether to colourise output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    /// Colour when stdout is a terminal and `NO_COLOR` is unset.
    #[default]
    Auto,
    Always,
    Never,
}

/// Styled output helpers.
#[derive(Debug, Clone, Copy)]
pub struct Ui {
    color: bool,
    quiet: bool,
}

impl Ui {
    pub fn new(choice: ColorChoice, quiet: bool) -> Self {
        let color = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
            }
        };
        Self { color, quiet }
    }

    /// Whether it is safe to ask the user a question.
    ///
    /// Both ends have to be a terminal: piping output into a file or a CI log
    /// means nobody would see the prompt, and a prompt nobody sees is a hang.
    pub fn interactive(self) -> bool {
        !self.quiet && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
    }

    /// Prints unless `--quiet` was given. Errors and warnings bypass this.
    pub fn say(self, line: impl AsRef<str>) {
        if !self.quiet {
            println!("{}", line.as_ref());
        }
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.color {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    pub fn green(self, text: &str) -> String {
        self.paint("32", text)
    }

    pub fn yellow(self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn red(self, text: &str) -> String {
        self.paint("31", text)
    }

    pub fn cyan(self, text: &str) -> String {
        self.paint("36", text)
    }
}

/// Pads `text` to `width` display columns.
///
/// Provider ids and paths are ASCII, so byte length is a correct column count
/// here and avoids pulling in a Unicode width dependency for table alignment.
pub fn pad(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(width.max(text.len()));
    out.push_str(text);
    for _ in text.len()..width {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_choice_emits_no_escape_sequences() {
        let ui = Ui::new(ColorChoice::Never, false);
        assert_eq!(ui.green("ok"), "ok");
        assert_eq!(ui.bold("ok"), "ok");
    }

    #[test]
    fn always_choice_wraps_in_ansi() {
        let ui = Ui::new(ColorChoice::Always, false);
        assert_eq!(ui.green("ok"), "\u{1b}[32mok\u{1b}[0m");
    }

    #[test]
    fn pad_never_truncates() {
        assert_eq!(pad("abc", 5), "abc  ");
        assert_eq!(pad("abcdef", 3), "abcdef");
    }
}
