/// Individual syntax features that can be toggled independently.
///
/// Each field corresponds to a specific shell syntax extension beyond POSIX.
/// `ParseOptions::default()` gives POSIX-only (all `false`).
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// `<<<` here-strings.
    pub here_strings: bool,
    /// `&>` and `&>>` redirects (redirect stdout+stderr).
    pub ampersand_redirect: bool,
    /// `[[ ]]` extended test command.
    pub double_brackets: bool,
    /// `(( ))` arithmetic command.
    pub arithmetic_command: bool,
    /// `<()` and `>()` process substitution.
    pub process_substitution: bool,
    /// `;;&` and `;&` in case statements (fall-through).
    pub extended_case: bool,
    /// `var=(...)` indexed arrays.
    pub arrays: bool,
    /// `coproc` command.
    pub coproc: bool,
    /// `select` loop.
    pub select: bool,
    /// `function name { }` (without parentheses).
    pub function_keyword: bool,
    /// `{n..m}` brace expansion.
    pub brace_expansion: bool,
    /// `=~` regex match inside `[[ ]]`.
    pub regex_match: bool,
    /// `|&` pipe stderr.
    pub pipe_stderr: bool,
    /// `$'...'` ANSI-C quoting.
    pub ansi_c_quoting: bool,
    /// `$"..."` locale translation.
    pub locale_translation: bool,
    /// Extended globbing: `?(pat)`, `*(pat)`, `+(pat)`, `@(pat)`, `!(pat)`.
    pub extglob: bool,
    /// `for ((init; cond; update))` C-style for loop.
    pub arithmetic_for: bool,
}

/// A named set of parse options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// POSIX sh — no extensions.
    Posix,
    /// GNU Bash — all extensions enabled.
    Bash,
}

impl Dialect {
    /// Get the `ParseOptions` for this dialect.
    pub fn options(&self) -> ParseOptions {
        match self {
            Dialect::Posix => ParseOptions::default(),
            Dialect::Bash => ParseOptions {
                here_strings: true,
                ampersand_redirect: true,
                double_brackets: true,
                arithmetic_command: true,
                process_substitution: true,
                extended_case: true,
                arrays: true,
                coproc: true,
                select: true,
                function_keyword: true,
                brace_expansion: true,
                regex_match: true,
                pipe_stderr: true,
                ansi_c_quoting: true,
                locale_translation: true,
                extglob: true,
                arithmetic_for: true,
            },
        }
    }
}
