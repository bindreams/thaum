//! Feature-flag system for shell dialect differences.
//!
//! `ShellOptions` holds one bool per syntax extension (here-strings, `[[ ]]`,
//! process substitution, etc.). `Dialect` provides named presets (`Posix`,
//! `Bash`). The lexer and parser read these flags to decide which constructs
//! to recognize.

/// Individual syntax features that can be toggled independently.
///
/// Each field corresponds to a specific shell syntax extension beyond POSIX.
/// `ShellOptions::default()` gives POSIX-only (all `false`).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShellOptions {
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
    /// Allow empty compound bodies (`if true; then fi`, `while false; do done`).
    pub empty_compound_body: bool,

    // Execution-specific flags --------------------------------------------------------
    /// `declare` / `typeset` builtins (Bash).
    pub declare_builtin: bool,
    /// `shopt` builtin (Bash).
    pub shopt_builtin: bool,
    /// `local` builtin (non-POSIX but universal — dash, bash, zsh all have it).
    pub local_builtin: bool,
    /// `declare -A` associative arrays (Bash).
    pub assoc_arrays: bool,
    /// `declare -n` namerefs (Bash 4.3+).
    pub nameref: bool,
    /// `declare -i` integer attribute (Bash).
    pub integer_attr: bool,
    /// `declare -l` / `declare -u` case conversion attributes (Bash 4+).
    pub case_attrs: bool,
    /// `${var^}`, `${var^^}`, `${var,}`, `${var,,}` case modification (Bash 4+).
    pub case_modification: bool,
    /// `${var@Q}`, `${var@a}`, etc. parameter transformation (Bash 4.4+).
    pub parameter_transform: bool,
    /// `@L`/`@U`/`@u`/`@K`/`@k` parameter transformations (Bash 5.1+).
    pub parameter_transform_51: bool,
    /// Bash 4.x bug: `"${a[@]:+word}"` on array with single empty element
    /// incorrectly returns word instead of empty. Fixed in bash 5.0.
    pub array_empty_element_alternative_bug: bool,
}

/// A named set of shell options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// POSIX sh — no extensions.
    Posix,
    /// Debian Almquist Shell — POSIX plus `local`.
    Dash,
    /// Bash 4.4: `@Q`/`@a`/`@A`/`@E`/`@P` transforms, has array empty-element bug.
    Bash44,
    /// Bash 5.0: empty-element bug fixed, no `@L`/`@U`/`@u`/`@K`/`@k` yet.
    Bash50,
    /// Bash 5.1: adds `@L`/`@U`/`@u`/`@K`/`@k` transforms.
    Bash51,
    /// Alias for the latest supported Bash version (currently `Bash51`).
    Bash,
}

impl Dialect {
    /// Get the `ShellOptions` for this dialect.
    pub fn options(&self) -> ShellOptions {
        match self {
            Dialect::Posix => ShellOptions::default(),
            Dialect::Dash => ShellOptions {
                local_builtin: true,
                ..ShellOptions::default()
            },
            Dialect::Bash44 => {
                let mut opts = Dialect::Bash51.options();
                opts.array_empty_element_alternative_bug = true;
                opts.parameter_transform_51 = false;
                opts
            }
            Dialect::Bash50 => {
                let mut opts = Dialect::Bash51.options();
                opts.parameter_transform_51 = false;
                opts
            }
            Dialect::Bash51 | Dialect::Bash => ShellOptions {
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
                empty_compound_body: true,
                declare_builtin: true,
                shopt_builtin: true,
                local_builtin: true,
                assoc_arrays: true,
                nameref: true,
                integer_attr: true,
                case_attrs: true,
                case_modification: true,
                parameter_transform: true,
                parameter_transform_51: true,
                array_empty_element_alternative_bug: false,
            },
        }
    }
}
