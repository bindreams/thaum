//! Free-function fragment helpers shared between the parser and the executor's
//! gettext re-lex path.
//!
//! `token_to_fragment` and `lex_double_quoted_content` only depend on
//! `&ShellOptions` (no other parser state), so they live here as free
//! functions. `Parser::token_to_fragment` becomes a thin forwarding wrapper.
//! `parse_brace_param_content` (in `params.rs`) and the executor's gettext
//! expansion path also call `token_to_fragment` directly, where no `Parser`
//! instance is available.

use crate::ast::*;
use crate::dialect::ShellOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::parser::helpers::de_escape_literal;
use crate::token::{ExtGlobTokenKind, GlobKind, SpannedToken, Token};

/// Convert a single fragment-bearing token to a `Fragment` AST node. Recurses
/// into command-substitution and parameter-expansion bodies via the parser.
pub(crate) fn token_to_fragment(st: SpannedToken, options: &ShellOptions) -> Result<Fragment, ParseError> {
    match st.token {
        Token::Literal(s) => Ok(Fragment::Literal(de_escape_literal(&s))),
        Token::SingleQuoted(s) => Ok(Fragment::SingleQuoted(s)),
        Token::DoubleQuoted(raw) => {
            let inner = lex_double_quoted_content(&raw, options)?;
            Ok(Fragment::DoubleQuoted(inner))
        }
        Token::SimpleParam(name) => Ok(Fragment::Parameter(ParameterExpansion::Simple(name))),
        Token::BraceParam(raw) => {
            let expansion = crate::word::parse_brace_param_content(&raw, options)?;
            Ok(Fragment::Parameter(expansion))
        }
        Token::CommandSub(raw) => {
            let stmts = crate::word::parse_command_substitution(&raw, options.clone())?;
            Ok(Fragment::CommandSubstitution(stmts))
        }
        Token::BacktickSub(raw) => {
            let stmts = crate::word::parse_command_substitution(&raw, options.clone())?;
            Ok(Fragment::CommandSubstitution(stmts))
        }
        Token::ArithSub(raw) => {
            // TODO: parse_arith_expr errors are silently swallowed here. Same
            // class of bug as the pre-fix parse_command_substitution; out of
            // scope for the heredoc fix.
            #[allow(clippy::unnecessary_lazy_evaluations)]
            let arith = crate::parser::arith_expr::parse_arith_expr(&raw).unwrap_or_else(|_| ArithExpr::Variable(raw));
            Ok(Fragment::ArithmeticExpansion(arith))
        }
        Token::Glob(kind) => {
            let gc = match kind {
                GlobKind::Star => GlobChar::Star,
                GlobKind::Question => GlobChar::Question,
                GlobKind::BracketOpen => GlobChar::BracketOpen,
            };
            Ok(Fragment::Glob(gc))
        }
        Token::TildePrefix(user) => Ok(Fragment::TildePrefix(user)),
        Token::BashAnsiCQuoted(content) => Ok(Fragment::BashAnsiCQuoted(content)),
        Token::BashLocaleQuoted(raw) => {
            let inner = lex_double_quoted_content(&raw, options)?;
            Ok(Fragment::BashLocaleQuoted { raw, parts: inner })
        }
        Token::BashExtGlob { kind, pattern } => {
            let ast_kind = match kind {
                ExtGlobTokenKind::ZeroOrOne => ExtGlobKind::ZeroOrOne,
                ExtGlobTokenKind::ZeroOrMore => ExtGlobKind::ZeroOrMore,
                ExtGlobTokenKind::OneOrMore => ExtGlobKind::OneOrMore,
                ExtGlobTokenKind::ExactlyOne => ExtGlobKind::ExactlyOne,
                ExtGlobTokenKind::Not => ExtGlobKind::Not,
            };
            Ok(Fragment::BashExtGlob {
                kind: ast_kind,
                pattern,
            })
        }
        Token::BashProcessSub { content, .. } => {
            let stmts = crate::word::parse_command_substitution(&content, options.clone())?;
            Ok(Fragment::CommandSubstitution(stmts))
        }
        _ => unreachable!("token_to_fragment called with non-fragment token: {:?}", st.token),
    }
}

/// Lex the inner content of a double-quoted string into fragments. Spawns an
/// inner `Lexer` in `LexerMode::DoubleQuote` over `raw`.
pub(crate) fn lex_double_quoted_content(raw: &str, options: &ShellOptions) -> Result<Vec<Fragment>, ParseError> {
    let mut inner_lexer = Lexer::new_double_quote_mode(raw, options.clone());
    let mut fragments = Vec::new();
    loop {
        let tok = inner_lexer.next_token()?;
        if tok.token == Token::Eof {
            break;
        }
        let frag = token_to_fragment(tok, options)?;
        fragments.push(frag);
    }
    Ok(fragments)
}

/// Merge adjacent `Fragment::Literal` values into single fragments. Used by
/// the parser's `collect_word` and by `parse_brace_arg` for cleaner ASTs.
pub(crate) fn merge_adjacent_literals(fragments: Vec<Fragment>) -> Vec<Fragment> {
    let mut result: Vec<Fragment> = Vec::with_capacity(fragments.len());
    for frag in fragments {
        if let Fragment::Literal(s) = &frag {
            if let Some(Fragment::Literal(prev)) = result.last_mut() {
                prev.push_str(s);
                continue;
            }
        }
        result.push(frag);
    }
    result
}
