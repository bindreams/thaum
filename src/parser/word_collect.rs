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
                let expansion = crate::word::parse_brace_param_content(
                    &raw,
                    self.options.case_modification,
                    self.options.parameter_transform,
                    self.options.parameter_transform_51,
                );
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

/// Detect brace expansions, both within single Literal fragments and
/// spanning across non-literal fragments (e.g., `{$a,b}`).
///
/// Recursively processes the result so adjacent and nested braces are
/// all detected.
fn detect_brace_expansions(fragments: Vec<Fragment>) -> Vec<Fragment> {
    // Pass 1: braces entirely within individual Literal fragments.
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

    // Pass 2: braces spanning across fragments (e.g., {$a,b}).
    detect_cross_fragment_braces(result)
}

/// Detect brace expansions that span across multiple fragments.
///
/// Looks for `{` at the end of a Literal and `}` in a later Literal,
/// with commas at depth 0 separating list items. Each item becomes a
/// `Vec<Fragment>` which may contain non-Literal fragments.
fn detect_cross_fragment_braces(fragments: Vec<Fragment>) -> Vec<Fragment> {
    // Find the first `{` inside a Literal that isn't already part of a
    // BashBraceExpansion and doesn't have a matching `}` in the same Literal.
    let Some((open_idx, open_pos)) = find_unmatched_open_brace(&fragments) else {
        return fragments;
    };

    // Scan forward for the matching `}`.
    let Some((close_idx, close_pos)) = find_matching_close_brace(&fragments, open_idx, open_pos) else {
        return fragments;
    };

    // Collect all fragments between the braces and split at depth-0 commas.
    let Some(items) = split_cross_fragment_items(&fragments, open_idx, open_pos, close_idx, close_pos) else {
        return fragments;
    };

    // Must have at least 2 items (at least one comma).
    if items.len() < 2 {
        return fragments;
    }

    // Recursively detect brace expansions in each item.
    let items: Vec<Vec<Fragment>> = items.into_iter().map(detect_brace_expansions).collect();

    // Build the result: prefix fragments + BraceExpansion + suffix fragments.
    let mut result = Vec::new();

    // Fragments before the one containing `{`.
    result.extend(fragments[..open_idx].iter().cloned());

    // Prefix: text before `{` in its Literal.
    if let Fragment::Literal(ref s) = fragments[open_idx] {
        let prefix = &s[..open_pos];
        if !prefix.is_empty() {
            result.push(Fragment::Literal(prefix.to_string()));
        }
    }

    result.push(Fragment::BashBraceExpansion(BraceExpansionKind::List(items)));

    // Suffix: text after `}` in its Literal.
    if let Fragment::Literal(ref s) = fragments[close_idx] {
        let suffix = &s[close_pos + 1..];
        if !suffix.is_empty() {
            result.push(Fragment::Literal(suffix.to_string()));
        }
    }

    // Fragments after the one containing `}`.
    result.extend(fragments[close_idx + 1..].iter().cloned());

    // Recurse: there may be more cross-fragment braces or adjacent expansions.
    detect_cross_fragment_braces(result)
}

/// Find the first unmatched `{` in a Literal fragment.
///
/// Returns `(fragment_index, char_position_within_literal)`.
/// Skips `{` characters that have a matching `}` in the same Literal
/// (those are handled by pass 1).
fn find_unmatched_open_brace(fragments: &[Fragment]) -> Option<(usize, usize)> {
    for (i, frag) in fragments.iter().enumerate() {
        if let Fragment::Literal(s) = frag {
            // Find each `{` and check if it has a matching `}` in the same string.
            for (pos, _) in s.char_indices().filter(|&(_, c)| c == '{') {
                let rest = &s[pos + 1..];
                let mut depth = 1u32;
                let mut matched = false;
                for c in rest.chars() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                matched = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if !matched {
                    // This `{` has no matching `}` in the same Literal —
                    // candidate for cross-fragment expansion.
                    return Some((i, pos));
                }
            }
        }
    }
    None
}

/// Find the `}` that matches a `{` at `(open_idx, open_pos)`, scanning forward
/// through subsequent fragments.
fn find_matching_close_brace(fragments: &[Fragment], open_idx: usize, open_pos: usize) -> Option<(usize, usize)> {
    let mut depth = 1u32;

    // Scan the rest of the opening Literal (after the `{`).
    if let Fragment::Literal(s) = &fragments[open_idx] {
        for c in s[open_pos + 1..].chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        // Matched within the same Literal — should have been
                        // caught by pass 1. This shouldn't happen, but handle it.
                        return None;
                    }
                }
                _ => {}
            }
        }
    }

    // Scan subsequent fragments.
    for (i, frag) in fragments.iter().enumerate().skip(open_idx + 1) {
        if let Fragment::Literal(s) = frag {
            for (pos, c) in s.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((i, pos));
                        }
                    }
                    _ => {}
                }
            }
        }
        // Non-Literal fragments don't affect depth.
    }

    None
}

/// Split the content between `{` and `}` at depth-0 commas, producing
/// a list of fragment vectors.
fn split_cross_fragment_items(
    fragments: &[Fragment],
    open_idx: usize,
    open_pos: usize,
    close_idx: usize,
    close_pos: usize,
) -> Option<Vec<Vec<Fragment>>> {
    // Collect all "content fragments" between { and }.
    // The opening Literal contributes its text after `{`.
    // The closing Literal contributes its text before `}`.
    // Fragments in between are included wholly.
    let mut content: Vec<Fragment> = Vec::new();

    if open_idx == close_idx {
        // Same Literal — shouldn't reach here, but handle defensively.
        if let Fragment::Literal(s) = &fragments[open_idx] {
            let inner = &s[open_pos + 1..close_pos];
            if !inner.is_empty() {
                content.push(Fragment::Literal(inner.to_string()));
            }
        }
    } else {
        // Text after `{` in the opening Literal.
        if let Fragment::Literal(s) = &fragments[open_idx] {
            let after = &s[open_pos + 1..];
            if !after.is_empty() {
                content.push(Fragment::Literal(after.to_string()));
            }
        }
        // All fragments between open and close.
        for frag in &fragments[open_idx + 1..close_idx] {
            content.push(frag.clone());
        }
        // Text before `}` in the closing Literal.
        if let Fragment::Literal(s) = &fragments[close_idx] {
            let before = &s[..close_pos];
            if !before.is_empty() {
                content.push(Fragment::Literal(before.to_string()));
            }
        }
    }

    // Check for `..' in literal-only content (sequence).
    // Cross-fragment sequences don't make sense (e.g., {$a..$b}), so skip.

    // Split content at depth-0 commas inside Literal fragments.
    split_fragments_at_commas(&content)
}

/// Split a fragment list at depth-0 commas within Literal fragments.
///
/// Returns `None` if no commas are found (not a valid list expansion).
fn split_fragments_at_commas(content: &[Fragment]) -> Option<Vec<Vec<Fragment>>> {
    let mut items: Vec<Vec<Fragment>> = vec![vec![]];
    let mut depth = 0u32;

    for frag in content {
        if let Fragment::Literal(s) = frag {
            let mut start = 0;
            for (pos, c) in s.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        // Emit text before the comma into the current item.
                        let before = &s[start..pos];
                        if !before.is_empty() {
                            items.last_mut().unwrap().push(Fragment::Literal(before.to_string()));
                        }
                        // Start a new item.
                        items.push(vec![]);
                        start = pos + 1;
                    }
                    _ => {}
                }
            }
            // Remaining text after the last comma (or the whole string if no commas).
            let rest = &s[start..];
            if !rest.is_empty() {
                items.last_mut().unwrap().push(Fragment::Literal(rest.to_string()));
            }
        } else {
            // Non-literal fragment goes into the current item as-is.
            items.last_mut().unwrap().push(frag.clone());
        }
    }

    if items.len() < 2 {
        None
    } else {
        Some(items)
    }
}

/// Try to find and split a brace expansion in a literal string.
///
/// On success returns a fragment list: `[prefix?, BashBraceExpansion, suffix_fragments...]`.
/// The suffix is recursively processed for adjacent brace expansions,
/// and list items are recursively processed for nested braces.
fn try_split_brace_expansion(s: &str) -> Option<Vec<Fragment>> {
    // Try each `{` in the string — skip invalid pairs and try the next.
    let mut search_from = 0;
    while let Some(offset) = s[search_from..].find('{') {
        let brace_start = search_from + offset;
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
        let Some(brace_end) = brace_end else {
            break; // No matching `}`
        };
        let content = &s[brace_start + 1..brace_end];

        // Check for depth-0 commas first — if present, this is a comma list
        // regardless of whether `..` appears in the content.
        let has_depth0_comma = has_comma_at_depth_zero(content);

        let brace_kind = if has_depth0_comma {
            parse_comma_list(content)
        } else if content.contains("..") {
            parse_sequence(content)
        } else {
            None
        };

        if let Some(kind) = brace_kind {
            let mut result = Vec::new();
            let prefix = &s[..brace_start];
            if !prefix.is_empty() {
                result.push(Fragment::Literal(prefix.to_string()));
            }
            result.push(Fragment::BashBraceExpansion(kind));

            // Recursively process suffix for adjacent brace expansions.
            let suffix = &s[brace_end + 1..];
            if !suffix.is_empty() {
                match try_split_brace_expansion(suffix) {
                    Some(suffix_frags) => result.extend(suffix_frags),
                    None => result.push(Fragment::Literal(suffix.to_string())),
                }
            }
            return Some(result);
        }

        // This `{...}` pair wasn't a valid expansion; try next `{`.
        search_from = brace_start + 1;
    }
    None
}

/// Check if a brace content string has a comma at depth 0.
fn has_comma_at_depth_zero(content: &str) -> bool {
    let mut depth = 0;
    for c in content.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// Parse a `..`-separated sequence: `start..end[..step]`.
fn parse_sequence(content: &str) -> Option<BraceExpansionKind> {
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
}

/// Parse a comma-separated list, splitting only at depth-0 commas.
///
/// Nested braces inside items are recursively detected. For example,
/// `{A,={a,b}=,B}` splits into items `A`, `={a,b}=`, `B`, and the
/// middle item is further expanded to `[Lit("="), BraceExp, Lit("=")]`.
fn parse_comma_list(content: &str) -> Option<BraceExpansionKind> {
    // Split at depth-0 commas.
    let mut items: Vec<&str> = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in content.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&content[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&content[start..]);

    // Must have at least 2 items (at least one comma at depth 0).
    if items.len() < 2 {
        return None;
    }

    let item_fragments: Vec<Vec<Fragment>> = items
        .into_iter()
        .map(|item| {
            if item.is_empty() {
                vec![]
            } else {
                // Recursively detect nested brace expansions in each item.
                match try_split_brace_expansion(item) {
                    Some(frags) => frags,
                    None => vec![Fragment::Literal(item.to_string())],
                }
            }
        })
        .collect();
    Some(BraceExpansionKind::List(item_fragments))
}
