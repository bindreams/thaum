mod common;

use common::*;
use shell_parser::ast::*;

#[test]
fn stderr_redirect() {
    let cmd = first_cmd("cmd 2>/dev/null");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(2));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Output(_)));
}

#[test]
fn dup_stderr_to_stdout() {
    let cmd = first_cmd("cmd 2>&1");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(2));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::DupOutput(_)));
}

#[test]
fn input_and_output_redirect() {
    let cmd = first_cmd("sort < input.txt > output.txt");
    assert_eq!(cmd.redirects.len(), 2);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    assert!(matches!(&cmd.redirects[1].kind, RedirectKind::Output(_)));
}

#[test]
fn multiple_redirects_on_one_command() {
    let cmd = first_cmd("cmd < input > output 2>> errors");
    assert_eq!(cmd.redirects.len(), 3);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    assert!(matches!(&cmd.redirects[1].kind, RedirectKind::Output(_)));
    assert_eq!(cmd.redirects[2].fd, Some(2));
    assert!(matches!(&cmd.redirects[2].kind, RedirectKind::Append(_)));
}

#[test]
fn clobber_redirect() {
    let cmd = first_cmd("cmd >| file");
    assert_eq!(cmd.redirects.len(), 1);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Clobber(_)));
}

#[test]
fn read_write_redirect() {
    let cmd = first_cmd("cmd 3<> /dev/tcp/host/80");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(3));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::ReadWrite(_)));
}

#[test]
fn heredoc_basic() {
    let cmd = first_cmd("cat <<EOF\nhello\nworld\nEOF\n");
    assert_eq!(cmd.redirects.len(), 1);
    if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
        assert_eq!(body, "hello\nworld\n");
    } else {
        panic!("expected heredoc");
    }
}

#[test]
fn heredoc_quoted_delimiter() {
    let cmd = first_cmd("cat <<'END'\n$var\n$(cmd)\nEND\n");
    if let RedirectKind::HereDoc { quoted, body, .. } = &cmd.redirects[0].kind {
        assert!(quoted);
        assert_eq!(body, "$var\n$(cmd)\n");
    } else {
        panic!("expected heredoc");
    }
}

#[test]
fn heredoc_strip_tabs() {
    let cmd = first_cmd("cat <<-EOF\n\thello\n\tworld\n\tEOF\n");
    assert_eq!(cmd.redirects.len(), 1);
    if let RedirectKind::HereDoc {
        strip_tabs, body, ..
    } = &cmd.redirects[0].kind
    {
        assert!(strip_tabs);
        assert_eq!(body, "hello\nworld\n");
    } else {
        panic!("expected heredoc");
    }
}

#[test]
fn heredoc_with_separate_lines() {
    // Heredoc where the body is on a separate line from the command
    let prog = parse_ok("cat <<EOF\nhello\nEOF\necho after\n");
    assert_eq!(prog.statements.len(), 2);
    if let Expression::Command(cmd) = &prog.statements[0].expression {
        if let RedirectKind::HereDoc { body, .. } = &cmd.redirects[0].kind {
            assert_eq!(body, "hello\n");
        } else {
            panic!("expected heredoc");
        }
    } else {
        panic!("expected Command");
    }
}

#[test]
fn heredoc_inside_if() {
    // Heredocs inside compound commands must work — the body is consumed
    // as part of statement termination, not by parse_compound_list.
    let prog = parse_ok("if true; then\ncat <<EOF\nhello\nEOF\necho after\nfi\n");
    if let Expression::Compound {
        body: CompoundCommand::IfClause { then_body, .. },
        ..
    } = &prog.statements[0].expression
    {
        assert_eq!(then_body.len(), 2);
        if let Expression::Command(cmd) = &then_body[0].expression {
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

#[test]
fn heredoc_inside_while() {
    let prog = parse_ok("while true; do\ncat <<EOF\nhello\nEOF\nbreak\ndone\n");
    if let Expression::Compound {
        body: CompoundCommand::WhileClause { body, .. },
        ..
    } = &prog.statements[0].expression
    {
        assert_eq!(body.len(), 2); // cat with heredoc + break
    } else {
        panic!("expected WhileClause");
    }
}

#[test]
fn heredoc_with_redirect_inside_function() {
    // The pattern from dockerd-rootless-setuptool.sh
    let input = "f() {\n\tcat <<- EOT > /tmp/out\n\t\thello\n\tEOT\n\techo done\n}\n";
    let prog = shell_parser::parse_with(input, shell_parser::Dialect::Bash).unwrap();
    if let Expression::FunctionDef(f) = &prog.statements[0].expression {
        if let CompoundCommand::BraceGroup { body, .. } = f.body.as_ref() {
            assert_eq!(body.len(), 2); // cat with heredoc + echo
        } else {
            panic!("expected BraceGroup");
        }
    } else {
        panic!("expected FunctionDef");
    }
}

#[test]
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

#[test]
fn heredoc_with_or_rhs_after_body() {
    // When `||` appears on the same line as `<<EOF`, the RHS command may
    // follow after the heredoc body. The heredoc body should be transparent
    // to the || operator.
    // Source: /usr/share/doc/git/contrib/vscode/init.sh
    let input = "cat <<EOF ||\nhello world\nEOF\necho \"heredoc failed\"";
    let prog = parse_ok(input);
    assert!(matches!(
        &prog.statements[0].expression,
        Expression::Or { .. }
    ));
}

#[test]
fn heredoc_with_or_rhs_same_line() {
    // Sanity check: when the RHS is on the same line as ||, it works.
    let input = "cat <<EOF || echo \"heredoc failed\"\nhello world\nEOF";
    let prog = parse_ok(input);
    assert!(matches!(
        &prog.statements[0].expression,
        Expression::Or { .. }
    ));
}

#[test]
fn heredoc_with_and_rhs_after_body() {
    // Same issue with && instead of ||.
    let input = "cat <<EOF &&\nhello world\nEOF\necho \"next\"";
    let prog = parse_ok(input);
    assert!(matches!(
        &prog.statements[0].expression,
        Expression::And { .. }
    ));
}
