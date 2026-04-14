use crate::common::*;
use thaum::ast::*;
use thaum::parse;

#[skuld::test]
fn stderr_redirect() {
    let cmd = first_cmd("cmd 2>/dev/null");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(2));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Output(_)));
}

#[skuld::test]
fn dup_stderr_to_stdout() {
    let cmd = first_cmd("cmd 2>&1");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(2));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::DupOutput(_)));
}

#[skuld::test]
fn input_and_output_redirect() {
    let cmd = first_cmd("sort < input.txt > output.txt");
    assert_eq!(cmd.redirects.len(), 2);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    assert!(matches!(&cmd.redirects[1].kind, RedirectKind::Output(_)));
}

#[skuld::test]
fn multiple_redirects_on_one_command() {
    let cmd = first_cmd("cmd < input > output 2>> errors");
    assert_eq!(cmd.redirects.len(), 3);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    assert!(matches!(&cmd.redirects[1].kind, RedirectKind::Output(_)));
    assert_eq!(cmd.redirects[2].fd, Some(2));
    assert!(matches!(&cmd.redirects[2].kind, RedirectKind::Append(_)));
}

#[skuld::test]
fn clobber_redirect() {
    let cmd = first_cmd("cmd >| file");
    assert_eq!(cmd.redirects.len(), 1);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Clobber(_)));
}

#[skuld::test]
fn read_write_redirect() {
    let cmd = first_cmd("cmd 3<> /dev/tcp/host/80");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(3));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::ReadWrite(_)));
}

#[skuld::test]
fn heredoc_basic() {
    let cmd = first_cmd("cat <<EOF\nhello\nworld\nEOF\n");
    assert_eq!(cmd.redirects.len(), 1);
    if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
        assert_eq!(body, "hello\nworld\n");
    } else {
        panic!("expected heredoc");
    }
}

#[skuld::test]
fn heredoc_quoted_delimiter() {
    let cmd = first_cmd("cat <<'END'\n$var\n$(cmd)\nEND\n");
    if let RedirectKind::HereDoc { quoted, body, .. } = &cmd.redirects[0].kind {
        assert!(quoted);
        assert_eq!(body, "$var\n$(cmd)\n");
    } else {
        panic!("expected heredoc");
    }
}

#[skuld::test]
fn heredoc_strip_tabs() {
    let cmd = first_cmd("cat <<-EOF\n\thello\n\tworld\n\tEOF\n");
    assert_eq!(cmd.redirects.len(), 1);
    if let RedirectKind::HereDoc { strip_tabs, body, .. } = &cmd.redirects[0].kind {
        assert!(strip_tabs);
        assert_eq!(body, "hello\nworld\n");
    } else {
        panic!("expected heredoc");
    }
}

#[skuld::test]
fn heredoc_with_separate_lines() {
    // Heredoc where the body is on a separate line from the command
    let prog = parse_ok("cat <<EOF\nhello\nEOF\necho after\n");
    assert_eq!(prog.lines.len(), 2);
    if let Expression::Command(cmd) = &prog.lines[0][0].expression {
        if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
            assert_eq!(body, "hello\n");
        } else {
            panic!("expected heredoc");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn heredoc_inside_if() {
    // Heredocs inside compound commands must work — the body is consumed
    // as part of statement termination, not by parse_compound_list.
    let prog = parse_ok("if true; then\ncat <<EOF\nhello\nEOF\necho after\nfi\n");
    if let Expression::Compound {
        body: CompoundCommand::IfClause { then_body, .. },
        ..
    } = &prog.lines[0][0].expression
    {
        assert_eq!(then_body.iter().flatten().count(), 2);
        if let Expression::Command(cmd) = &then_body[0][0].expression {
            if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
                assert_eq!(body, "hello\n");
            } else {
                panic!("expected heredoc");
            }
        } else {
            panic!("expected Command");
        }
    } else {
        panic!("expected IfClause");
    }
}

#[skuld::test]
fn heredoc_inside_while() {
    let prog = parse_ok("while true; do\ncat <<EOF\nhello\nEOF\nbreak\ndone\n");
    if let Expression::Compound {
        body: CompoundCommand::WhileClause { body, .. },
        ..
    } = &prog.lines[0][0].expression
    {
        assert_eq!(body.iter().flatten().count(), 2); // cat with heredoc + break
    } else {
        panic!("expected WhileClause");
    }
}

#[skuld::test]
fn heredoc_with_redirect_inside_function() {
    // The pattern from dockerd-rootless-setuptool.sh
    let input = "f() {\n\tcat <<- EOT > /tmp/out\n\t\thello\n\tEOT\n\techo done\n}\n";
    let prog = thaum::parse_with(input, thaum::Dialect::Bash).unwrap();
    if let Expression::FunctionDef(f) = &prog.lines[0][0].expression {
        if let CompoundCommand::BraceGroup { body, .. } = f.body.as_ref() {
            assert_eq!(body.iter().flatten().count(), 2); // cat with heredoc + echo
        } else {
            panic!("expected BraceGroup");
        }
    } else {
        panic!("expected FunctionDef");
    }
}

#[skuld::test]
fn multiple_heredocs_on_one_line() {
    let input = "cmd <<A <<B\nbody1\nA\nbody2\nB\n";
    let cmd = first_cmd(input);
    assert_eq!(cmd.redirects.len(), 2);
    if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
        assert_eq!(body, "body1\n");
    } else {
        panic!("expected first heredoc");
    }
    if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[1].kind {
        assert_eq!(body, "body2\n");
    } else {
        panic!("expected second heredoc");
    }
}

#[skuld::test]
fn heredoc_with_or_rhs_after_body() {
    // When `||` appears on the same line as `<<EOF`, the RHS command may
    // follow after the heredoc body. The heredoc body should be transparent
    // to the || operator.
    // Source: /usr/share/doc/git/contrib/vscode/init.sh
    let input = "cat <<EOF ||\nhello world\nEOF\necho \"heredoc failed\"";
    let prog = parse_ok(input);
    assert!(matches!(&prog.lines[0][0].expression, Expression::Or { .. }));
}

#[skuld::test]
fn heredoc_with_or_rhs_same_line() {
    // Sanity check: when the RHS is on the same line as ||, it works.
    let input = "cat <<EOF || echo \"heredoc failed\"\nhello world\nEOF";
    let prog = parse_ok(input);
    assert!(matches!(&prog.lines[0][0].expression, Expression::Or { .. }));
}

#[skuld::test]
fn heredoc_with_and_rhs_after_body() {
    // Same issue with && instead of ||.
    let input = "cat <<EOF &&\nhello world\nEOF\necho \"next\"";
    let prog = parse_ok(input);
    assert!(matches!(&prog.lines[0][0].expression, Expression::And { .. }));
}

#[skuld::test]
fn heredoc_in_if_condition() {
    // Heredoc body appears between the condition line and `then`.
    let input = "if cat <<EOF; then\nhello\nEOF\necho yes\nfi";
    let prog = parse(input).unwrap();
    // The program should parse and the heredoc body should be filled.
    let expr = &prog.lines[0][0].expression;
    if let Expression::Compound {
        body: CompoundCommand::IfClause {
            condition, then_body, ..
        },
        ..
    } = expr
    {
        // Condition: cat <<EOF with body filled
        if let Expression::Command(cmd) = &condition[0][0].expression {
            if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
                assert_eq!(body, "hello\n");
            } else {
                panic!("expected heredoc redirect");
            }
        } else {
            panic!("expected command in condition");
        }
        // Then body: echo yes
        assert_eq!(then_body.iter().flatten().count(), 1);
    } else {
        panic!("expected if clause");
    }
}

#[skuld::test]
fn heredoc_with_pipe_on_last_line() {
    // Pipe on the heredoc-triggering line.
    let input = "cat <<EOF |\n1\n2\nEOF\ntac";
    let prog = parse(input).unwrap();
    if let Expression::Pipe { left, .. } = &prog.lines[0][0].expression {
        if let Expression::Command(cmd) = left.as_ref() {
            if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
                assert_eq!(body, "1\n2\n");
            } else {
                panic!("expected heredoc redirect");
            }
        } else {
            panic!("expected command on left side of pipe");
        }
    } else {
        panic!("expected pipe");
    }
}

#[skuld::test]
fn multiple_heredocs_in_pipeline() {
    let input = "cat <<A |\na\nA\ncat <<B\nb\nB";
    let prog = parse(input).unwrap();
    if let Expression::Pipe { left, right, .. } = &prog.lines[0][0].expression {
        // Left: cat <<A with body "a\n"
        if let Expression::Command(cmd) = left.as_ref() {
            if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
                assert_eq!(body, "a\n");
            } else {
                panic!("expected heredoc on left");
            }
        }
        // Right: cat <<B with body "b\n"
        if let Expression::Command(cmd) = right.as_ref() {
            if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
                assert_eq!(body, "b\n");
            } else {
                panic!("expected heredoc on right");
            }
        }
    } else {
        panic!("expected pipe");
    }
}

// Heredocs inside $(...) command substitution =========================================================================

/// Walk a Program down to the first heredoc inside the first arg's $(...) cmdsub.
fn inner_heredoc_body(prog: &Program) -> (String, bool) {
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        other => panic!("expected Command, got {other:?}"),
    };
    for arg in &cmd.arguments {
        if let Argument::Word(w) = arg {
            for frag in &w.parts {
                if let Fragment::CommandSubstitution(stmts) = frag {
                    let inner_cmd = match &stmts.first().expect("empty cmdsub stmts").expression {
                        Expression::Command(c) => c,
                        other => panic!("expected inner Command, got {other:?}"),
                    };
                    let redir = inner_cmd.redirects.first().expect("inner cmd has no redirects");
                    if let RedirectKind::HereDoc { body, quoted, .. } = &redir.kind {
                        return (body.clone(), *quoted);
                    }
                }
            }
        }
    }
    panic!("no heredoc-bearing cmdsub found");
}

/// Walk a Program down to a heredoc inside an inner $(..) of a brace-param's
/// default-value argument (i.e. `${var:-$(cat <<EOF\n..\nEOF\n)}`).
fn brace_default_inner_heredoc(prog: &Program) -> (String, bool) {
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        other => panic!("expected Command, got {other:?}"),
    };
    for arg in &cmd.arguments {
        if let Argument::Word(w) = arg {
            for frag in &w.parts {
                let inner = match frag {
                    Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
                    Fragment::DoubleQuoted(parts) => match parts.iter().find_map(|p| match p {
                        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => Some(a),
                        _ => None,
                    }) {
                        Some(a) => a,
                        None => continue,
                    },
                    _ => continue,
                };
                for inner_frag in &inner.parts {
                    if let Fragment::CommandSubstitution(stmts) = inner_frag {
                        let inner_cmd = match &stmts.first().expect("empty cmdsub stmts").expression {
                            Expression::Command(c) => c,
                            _ => panic!(),
                        };
                        let redir = inner_cmd.redirects.first().expect("inner cmd has no redirects");
                        if let RedirectKind::HereDoc { body, quoted, .. } = &redir.kind {
                            return (body.clone(), *quoted);
                        }
                    }
                }
            }
        }
    }
    panic!("no heredoc-bearing brace-param-default found");
}

#[skuld::test]
fn heredoc_in_cmdsub_body_has_single_quote() {
    // Regression: apostrophe in heredoc body caused "unterminated single quote".
    let prog = parse_ok("echo $(cat <<'EOF'\nit's good\nEOF\n)");
    let (body, quoted) = inner_heredoc_body(&prog);
    assert!(quoted);
    assert_eq!(body, "it's good\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_body_has_double_quote() {
    let prog = parse_ok("echo $(cat <<EOF\nhas \"quote\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "has \"quote\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_body_has_backtick() {
    let prog = parse_ok("echo $(cat <<'EOF'\nhas backtick `\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "has backtick `\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_body_has_close_paren() {
    // `)` in heredoc body must not close the cmdsub.
    let prog = parse_ok("echo $(cat <<EOF\nhas ) paren\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "has ) paren\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_inside_double_quoted() {
    // Original reported case: cmdsub inside a double-quoted string.
    let prog = parse_ok("echo \"$(cat <<'EOF'\nit's good\nEOF\n)\"");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let w = extract_word(&cmd.arguments[1]);
    let dq_parts = match &w.parts[0] {
        Fragment::DoubleQuoted(parts) => parts,
        other => panic!("expected DoubleQuoted, got {other:?}"),
    };
    let stmts = match &dq_parts[0] {
        Fragment::CommandSubstitution(s) => s,
        other => panic!("expected CommandSubstitution, got {other:?}"),
    };
    let inner_cmd = match &stmts[0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    match &inner_cmd.redirects[0].kind {
        RedirectKind::HereDoc { body, quoted, .. } => {
            assert!(*quoted);
            assert_eq!(body, "it's good\n");
        }
        _ => panic!("expected heredoc"),
    }
}

#[skuld::test]
fn multiple_heredocs_on_one_line_in_cmdsub() {
    let prog = parse_ok("echo $(cmd <<A <<B\nbody1 with 'quote'\nA\nbody2 with )\nB\n)");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let w = extract_word(&cmd.arguments[1]);
    let stmts = match &w.parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!(),
    };
    let inner = match &stmts[0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    assert_eq!(inner.redirects.len(), 2);
    match &inner.redirects[0].kind {
        RedirectKind::HereDoc { body, .. } => assert_eq!(body, "body1 with 'quote'\n"),
        _ => panic!(),
    }
    match &inner.redirects[1].kind {
        RedirectKind::HereDoc { body, .. } => assert_eq!(body, "body2 with )\n"),
        _ => panic!(),
    }
}

#[skuld::test]
fn nested_cmdsub_with_heredoc() {
    let prog = parse_ok("echo $(echo $(cat <<EOF\nbody with ) paren\nEOF\n))");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let outer_cs = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!(),
    };
    let mid_cmd = match &outer_cs[0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let inner_cs = match &extract_word(&mid_cmd.arguments[1]).parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!(),
    };
    let inner_cmd = match &inner_cs[0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    match &inner_cmd.redirects[0].kind {
        RedirectKind::HereDoc { body, .. } => assert_eq!(body, "body with ) paren\n"),
        _ => panic!(),
    }
}

#[skuld::test]
fn arith_left_shift_inside_cmdsub_not_heredoc() {
    // Regression guard: `<<` inside $((..)) inside $(...) must remain left-shift.
    let _ = parse_ok("echo $(echo $((1 << 2)))");
}

#[skuld::test]
fn arith_left_shift_with_spaces_inside_cmdsub_not_heredoc() {
    let _ = parse_ok("echo $(x=$(( 1 << 2 )); echo $x)");
}

#[skuld::test]
fn nested_arith_expressions_inside_cmdsub_not_heredoc() {
    // Deeply nested parens inside arith inside cmdsub.
    let _ = parse_ok("echo $(echo $((((1)) << 2)))");
}

#[skuld::test]
fn nested_subshell_with_space_in_cmdsub_allows_heredoc() {
    // `( (` with space is subshell-in-subshell — `<<EOF` inside the inner
    // subshell IS a heredoc operator and must be detected. Contrast with `((`
    // no-space which is arith and must NOT detect heredoc.
    let prog = parse_ok("echo $( ( cat <<EOF\nbody with )\nEOF\n) )");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let stmts = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!(),
    };
    assert!(!stmts.is_empty(), "inner cmdsub re-parse returned empty");
}

#[skuld::test]
fn double_open_paren_no_space_is_arith_inside_cmdsub() {
    // `((` with no space inside `$(...)` is arith. `<<` inside must be left-shift.
    let _ = parse_ok("echo $(((1 << 2)))");
}

#[skuld::test]
fn arith_with_nested_paren_groups_inside_cmdsub() {
    // Multiple paren groups inside arith, with `<<` inside each.
    let _ = parse_ok("echo $(echo $(( (1 << 2) + (3 << 4) )))");
}

#[skuld::test]
fn unterminated_heredoc_in_cmdsub_reports_heredoc_error() {
    // Error-quality check: should report heredoc-related error, not generic.
    let err = thaum::parse("echo $(cat <<EOF\nbody with no terminator").unwrap_err();
    let s = format!("{err}");
    assert!(
        s.contains("here-document") || s.contains("heredoc") || s.contains("HereDoc"),
        "expected heredoc-related error, got: {s}"
    );
}

#[skuld::test]
fn heredoc_in_cmdsub_body_has_backslash_newline() {
    // `\<newline>` in heredoc body is NOT a line continuation — it's literal.
    let prog = parse_ok("echo $(cat <<'EOF'\na\\\nb\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "a\\\nb\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_delimiter_substring_in_body() {
    // A line containing but not equal to the delimiter must not terminate.
    let prog = parse_ok("echo $(cat <<EOF\nprefix EOF suffix\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "prefix EOF suffix\n");
}

#[skuld::test]
fn empty_heredoc_body_in_cmdsub() {
    let prog = parse_ok("echo $(cat <<EOF\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "");
}

#[skuld::test]
fn heredoc_quoted_delimiter_double_quotes_in_cmdsub() {
    // <<"EOF" — delimiter is quoted, body is literal.
    let prog = parse_ok("echo $(cat <<\"EOF\"\nit's $literal\nEOF\n)");
    let (body, quoted) = inner_heredoc_body(&prog);
    assert!(quoted);
    assert_eq!(body, "it's $literal\n");
}

#[skuld::test]
fn heredoc_quoted_delimiter_backslash_in_cmdsub() {
    // <<\EOF — backslash-quoted delimiter, also marks the heredoc as quoted.
    let prog = parse_ok("echo $(cat <<\\EOF\nit's $literal\nEOF\n)");
    let (body, quoted) = inner_heredoc_body(&prog);
    assert!(quoted);
    assert_eq!(body, "it's $literal\n");
}

#[skuld::test]
fn heredoc_strip_tabs_with_mixed_leading_whitespace_in_cmdsub() {
    // <<- strips leading TABS only — leading spaces must be preserved.
    let prog = parse_ok("echo $(cat <<-EOF\n\t  text\n\tEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, "  text\n");
}

#[skuld::test]
fn multiple_arith_shifts_top_level_unaffected() {
    // Top-level arith shifts must be unaffected by the change.
    let _ = parse_ok("for big in $(( 1 << 32 )) $(( (1 << 63) - 1 )); do echo $big; done");
}

#[skuld::test]
fn multiple_top_level_arith_shifts_in_cmdsub() {
    // Multiple arith scopes on one line inside a cmdsub.
    let _ = parse_ok("echo $(echo $(( 1 << 2 )) and $(( 3 << 4 )))");
}

#[skuld::test]
fn close_paren_at_column_zero_in_heredoc_body_in_cmdsub() {
    // `)` at the start of a body line is preserved verbatim.
    let prog = parse_ok("echo $(cat <<EOF\n)\nEOF\n)");
    let (body, _) = inner_heredoc_body(&prog);
    assert_eq!(body, ")\n");
}

#[skuld::test]
fn case_with_heredoc_arm_inside_cmdsub_deep_assert() {
    // case_depth interacts with heredoc detection.
    let prog = parse_ok("echo $(case x in x) cat <<EOF\nbody with ) char\nEOF\n;; esac)");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let stmts = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!("expected CommandSubstitution"),
    };
    assert!(!stmts.is_empty(), "case-with-heredoc cmdsub re-parse returned empty");
}

#[skuld::test]
fn missing_heredoc_delimiter_in_cmdsub_reports_error() {
    // `<<` followed immediately by `\n` (no delimiter word) must error.
    let result = thaum::parse("echo $(cat <<\nbody\n)");
    assert!(result.is_err());
}

#[skuld::test]
fn empty_quoted_delimiter_in_cmdsub_terminated_by_blank_line() {
    // <<"" or <<'' is valid bash — body delimited by an empty line.
    let prog = parse_ok("echo $(cat <<\"\"\nbody1\nbody2\n\n)");
    let (body, quoted) = inner_heredoc_body(&prog);
    assert!(quoted);
    assert_eq!(body, "body1\nbody2\n");
}

// Heredoc inside backtick command substitution ========================================================================

#[skuld::test]
fn heredoc_in_backtick_with_metachars() {
    // Regression lock: backticks already handle heredocs correctly because
    // scan_backtick walks bytes flat without quote/paren tracking. Confirm
    // bodies with `'`, `)` round-trip through the inner re-parse.
    let prog = parse_ok("echo `cat <<'EOF'\nit's ) good\nEOF\n`");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let stmts = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::CommandSubstitution(s) => s,
        _ => panic!("expected backtick CommandSubstitution"),
    };
    let inner_cmd = match &stmts[0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    match &inner_cmd.redirects[0].kind {
        RedirectKind::HereDoc { body, quoted, .. } => {
            assert!(*quoted);
            assert_eq!(body, "it's ) good\n");
        }
        _ => panic!(),
    }
}

// parse_command_substitution error propagation ========================================================================

#[skuld::test]
fn syntax_error_inside_cmdsub_propagates() {
    // Pre-existing bug: parse_command_substitution silently swallowed errors
    // and `unwrap_or_default`-returned an empty Vec. After the fix, the error
    // must surface from the outer parse.
    let result = thaum::parse("echo $(if then fi)");
    assert!(result.is_err(), "expected parse error, got {result:?}");
}

// Brace-param structural argument parsing =============================================================================

#[skuld::test]
fn heredoc_in_cmdsub_inside_brace_param_default() {
    // ${var:-$(cat <<EOF\nit's body\nEOF\n)} — heredoc inside $(..) inside ${..}.
    let prog = parse_ok("echo ${var:-$(cat <<EOF\nit's body\nEOF\n)}");
    let (body, _) = brace_default_inner_heredoc(&prog);
    assert_eq!(body, "it's body\n");
}

#[skuld::test]
fn heredoc_in_cmdsub_inside_brace_param_default_with_metachars() {
    // Body has `'` and `)` to exercise heredoc detection through ${..} flat-scan.
    let prog = parse_ok("echo \"${var:-$(cat <<'EOF'\nit's ) good\nEOF\n)}\"");
    let (body, quoted) = brace_default_inner_heredoc(&prog);
    assert!(quoted);
    assert_eq!(body, "it's ) good\n");
}

#[skuld::test]
fn brace_param_default_with_simple_cmdsub_is_structural() {
    // ${var:-$(cmd)} must produce a structural CommandSubstitution Fragment,
    // not a flat Literal. Pre-existing bug regression.
    let prog = parse_ok("echo ${var:-$(echo defaulted)}");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let arg = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
        other => panic!("expected Complex parameter with argument, got {other:?}"),
    };
    assert!(
        matches!(arg.parts[0], Fragment::CommandSubstitution(_)),
        "brace-arg was flattened to literal: {:?}",
        arg.parts
    );
}

#[skuld::test]
fn brace_param_default_with_arith_is_structural() {
    // ${var:-$((1<<2))} — arith expansion. `<<` must not trigger heredoc.
    let prog = parse_ok("echo ${var:-$((1 << 2))}");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let arg = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
        other => panic!("expected Complex parameter with argument, got {other:?}"),
    };
    assert!(
        matches!(arg.parts[0], Fragment::ArithmeticExpansion(_)),
        "expected ArithmeticExpansion, got {:?}",
        arg.parts
    );
}

#[skuld::test]
fn brace_param_default_arith_inside_double_quoted() {
    // ${var:-$((1<<2))} inside "..."
    let _ = parse_ok("echo \"${var:-$((1 << 2))}\"");
}

#[skuld::test]
fn brace_param_trim_pattern_with_cmdsub_is_structural() {
    // ${var#$(echo prefix)} — trim pattern arg should also be structural.
    let prog = parse_ok("echo ${var#$(echo prefix)}");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let arg = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
        other => panic!("expected Complex parameter with argument, got {other:?}"),
    };
    assert!(
        matches!(arg.parts[0], Fragment::CommandSubstitution(_)),
        "trim-pattern arg was flattened to literal: {:?}",
        arg.parts
    );
}

#[skuld::test]
fn brace_param_trim_pattern_with_arith_is_structural() {
    // ${var#$((1<<2))} — trim pattern with arith expansion.
    let prog = parse_ok("echo ${var#$((1 << 2))}");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let arg = match &extract_word(&cmd.arguments[1]).parts[0] {
        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
        _ => panic!(),
    };
    assert!(matches!(arg.parts[0], Fragment::ArithmeticExpansion(_)));
}

#[skuld::test]
fn arith_inside_double_quoted_string_not_heredoc() {
    // DQ-mode path through scan_double_quoted must not trigger heredoc detection.
    let _ = parse_ok("echo \"$((1 << 2))\"");
}

#[skuld::test]
fn arith_inside_double_quoted_string_in_cmdsub_not_heredoc() {
    // arith inside DQ inside cmdsub. Multiple layers must each independently
    // decide enable_heredocs correctly.
    let _ = parse_ok("echo $(echo \"$((1 << 2))\")");
}

// Code-review regression guards (post-fix bug catches) ================================================================

#[skuld::test]
fn brace_param_arith_with_inner_paren_group_not_heredoc() {
    // Bug A: arith with paren group inside ${...} caused MissingHereDocDelimiter
    // because the consecutive-paren heuristic only fired when open=='('. Fixed
    // by recursive descent on $((..)) regardless of outer scope.
    let _ = parse_ok("echo \"${var:-$((1<<(1+1)))}\"");
    let _ = parse_ok("echo \"${var:-$((1<<(2)))}\"");
    let _ = parse_ok("echo \"${var:-$(((1+1) << 2))}\"");
}

#[skuld::test]
fn brace_param_literal_double_less_in_default() {
    // Bug B: literal `<<` inside ${...} brace-arg was misinterpreted as a
    // heredoc operator. Bash treats it as literal text. Fixed by setting
    // enable_heredocs=false at the brace-arg level (nested $(..) re-enables).
    let prog = parse_ok("echo \"${var:-<<EOF}\"");
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!(),
    };
    let w = extract_word(&cmd.arguments[1]);
    let dq_parts = match &w.parts[0] {
        Fragment::DoubleQuoted(parts) => parts,
        _ => panic!(),
    };
    let arg = match &dq_parts[0] {
        Fragment::Parameter(ParameterExpansion::Complex { argument: Some(a), .. }) => a,
        _ => panic!(),
    };
    // The brace-arg should be a literal "<<EOF", not a heredoc operator.
    match &arg.parts[0] {
        Fragment::Literal(s) => assert_eq!(s, "<<EOF"),
        other => panic!("expected literal, got {other:?}"),
    }
}

#[skuld::test]
fn brace_param_multiline_arith_default_not_heredoc() {
    // Bug C: multi-line brace-arg with arith caused spurious heredoc body read.
    // The newline after `$((1<<2))` triggered a heredoc body scan with
    // delimiter `2`. Fixed by recursive descent on $((..)).
    let _ = parse_ok("echo \"${var:-$((1 << 2))\nliteral stuff}\"");
}
