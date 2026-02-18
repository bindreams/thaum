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
