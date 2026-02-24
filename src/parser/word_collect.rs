//! Word collection: gathers adjacent fragment tokens into Word AST nodes.
//!
//! The lexer emits fragment-level tokens (Literal, SimpleParam, etc.) with
//! Whitespace tokens as word boundaries. This module provides Parser methods to
//! collect those fragments into structured Word and Argument nodes.

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{ExtGlobTokenKind, GlobKind, SpannedToken, Token};

use super::helpers::de_escape_literal;
use super::Parser;

impl Parser {
    /// Collect adjacent fragment tokens into a Word AST node.
    /// Uses the raw API to see Whitespace tokens as word boundaries.
    /// Returns None if the current token is not a fragment.
    pub(super) fn collect_word(&mut self) -> Result<Option<Word>, ParseError> {
        if !self.lexer.peek()?.token.is_fragment() {
            return Ok(None);
        }

        let start_span = self.lexer.peek()?.span;
        let mut fragments = Vec::new();
        let mut end_span = start_span;

        while self.lexer.peek()?.token.is_fragment() {
            let st = self.lexer.advance()?;
            end_span = st.span;
            let frag = self.token_to_fragment(st)?;
            fragments.push(frag);
        }

        let fragments = merge_adjacent_literals(fragments);

        let fragments = if self.options.brace_expansion {
            detect_brace_expansions(fragments)
        } else {
            fragments
        };

        Ok(Some(Word {
            parts: fragments,
            span: start_span.merge(end_span),
        }))
    }

    /// Collect adjacent fragment tokens into an Argument AST node.
    /// Handles BashProcessSub -> Atom conversion.
    pub(super) fn collect_argument(&mut self) -> Result<Option<Argument>, ParseError> {
        if let Token::BashProcessSub { .. } = &self.lexer.peek()?.token {
            let st = self.lexer.advance()?;
            if let Token::BashProcessSub { direction, content } = st.token {
                let dir = if direction == '<' {
                    ProcessDirection::In
                } else {
                    ProcessDirection::Out
                };
                let stmts = crate::word::parse_command_substitution(&content);
                return Ok(Some(Argument::Atom(Atom::BashProcessSubstitution {
                    direction: dir,
                    body: stmts,
                    span: st.span,
                })));
            }
        }

        match self.collect_word()? {
            Some(w) => Ok(Some(Argument::Word(w))),
            None => Ok(None),
        }
    }

    /// Collect remaining value fragments after an assignment `=`.
    pub(super) fn collect_assignment_value(
        &mut self,
        value_prefix: &str,
        start_span: Span,
    ) -> Result<Word, ParseError> {
        let mut fragments = Vec::new();
        let mut end_span = start_span;

        if !value_prefix.is_empty() {
            fragments.push(Fragment::Literal(de_escape_literal(value_prefix)));
        }

        while self.lexer.peek()?.token.is_fragment() {
            let st = self.lexer.advance()?;
            end_span = st.span;
            let frag = self.token_to_fragment(st)?;
            fragments.push(frag);
        }

        let fragments = merge_adjacent_literals(fragments);

        Ok(Word {
            parts: fragments,
            span: start_span.merge(end_span),
        })
    }

    /// Convert a single fragment token to a Fragment AST node.
    pub(super) fn token_to_fragment(&mut self, st: SpannedToken) -> Result<Fragment, ParseError> {
        match st.token {
            Token::Literal(s) => Ok(Fragment::Literal(de_escape_literal(&s))),
            Token::SingleQuoted(s) => Ok(Fragment::SingleQuoted(s)),
            Token::DoubleQuoted(raw) => {
                let inner = self.lex_double_quoted_content(&raw)?;
                Ok(Fragment::DoubleQuoted(inner))
            }
            Token::SimpleParam(name) => Ok(Fragment::Parameter(ParameterExpansion::Simple(name))),
            Token::BraceParam(raw) => {
                let expansion = crate::word::parse_brace_param_content(&raw, self.options.case_modification);
                Ok(Fragment::Parameter(expansion))
            }
            Token::CommandSub(raw) => {
                let stmts = crate::word::parse_command_substitution(&raw);
                Ok(Fragment::CommandSubstitution(stmts))
            }
            Token::BacktickSub(raw) => {
                let stmts = crate::word::parse_command_substitution(&raw);
                Ok(Fragment::CommandSubstitution(stmts))
            }
            Token::ArithSub(raw) => {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                let arith =
                    crate::parser::arith_expr::parse_arith_expr(&raw).unwrap_or_else(|_| ArithExpr::Variable(raw));
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
                let inner = self.lex_double_quoted_content(&raw)?;
                Ok(Fragment::BashLocaleQuoted(inner))
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
                let stmts = crate::word::parse_command_substitution(&content);
                Ok(Fragment::CommandSubstitution(stmts))
            }
            _ => unreachable!("token_to_fragment called with non-fragment token: {:?}", st.token),
        }
    }

    /// Lex the inner content of a double-quoted string into fragments.
    fn lex_double_quoted_content(&mut self, raw: &str) -> Result<Vec<Fragment>, ParseError> {
        let mut inner_lexer = Lexer::new_double_quote_mode(raw, self.options.clone());
        let mut fragments = Vec::new();
        loop {
            let tok = inner_lexer.next_token()?;
            if tok.token == Token::Eof {
                break;
            }
            let frag = self.token_to_fragment(tok)?;
            fragments.push(frag);
        }
        Ok(fragments)
    }
}

// Private helpers =====================================================================================================

/// Merge adjacent Literal fragments for cleaner ASTs.
fn merge_adjacent_literals(fragments: Vec<Fragment>) -> Vec<Fragment> {
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

/// Detect brace expansions within Literal fragments.
fn detect_brace_expansions(fragments: Vec<Fragment>) -> Vec<Fragment> {
    let mut result = Vec::with_capacity(fragments.len());
    for frag in fragments {
        if let Fragment::Literal(ref s) = frag {
            if let Some(expanded) = try_split_brace_expansion(s) {
                result.extend(expanded);
                continue;
            }
        }
        result.push(frag);
    }
    result
}

/// Try to find and split a brace expansion in a literal string.
fn try_split_brace_expansion(s: &str) -> Option<Vec<Fragment>> {
    let brace_start = s.find('{')?;
    let rest = &s[brace_start + 1..];
    let mut depth = 1;
    let mut brace_end = None;
    for (i, c) in rest.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    brace_end = Some(brace_start + 1 + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let brace_end = brace_end?;
    let content = &s[brace_start + 1..brace_end];

    let brace_kind = if content.contains("..") {
        let parts: Vec<&str> = content.splitn(3, "..").collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            Some(BraceExpansionKind::Sequence {
                start: parts[0].to_string(),
                end: parts[1].to_string(),
                step: parts.get(2).map(|s| s.to_string()),
            })
        } else {
            None
        }
    } else if content.contains(',') {
        let items: Vec<&str> = content.split(',').collect();
        let item_fragments: Vec<Vec<Fragment>> = items
            .into_iter()
            .map(|item| {
                if item.is_empty() {
                    vec![]
                } else {
                    vec![Fragment::Literal(item.to_string())]
                }
            })
            .collect();
        Some(BraceExpansionKind::List(item_fragments))
    } else {
        None
    };

    let brace_kind = brace_kind?;

    let mut result = Vec::new();
    let prefix = &s[..brace_start];
    if !prefix.is_empty() {
        result.push(Fragment::Literal(prefix.to_string()));
    }
    result.push(Fragment::BashBraceExpansion(brace_kind));
    let suffix = &s[brace_end + 1..];
    if !suffix.is_empty() {
        result.push(Fragment::Literal(suffix.to_string()));
    }
    Some(result)
}
