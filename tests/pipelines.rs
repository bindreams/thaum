mod common;

use common::*;
use shell_parser::ast::*;
use shell_parser::parse;

#[test]
fn pipeline_two_commands() {
    assert!(matches!(
        first_expr("ls | grep foo"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn pipeline_four_commands() {
    let e = first_expr("cat file.txt | grep pattern | sort | uniq -c");
    if let Expression::Pipe { right, .. } = &e {
        if let Expression::Command(c) = right.as_ref() {
            assert_eq!(word_literal(&c.arguments[0]), "uniq");
        } else {
            panic!("expected Command at right");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn pipeline_with_redirects() {
    let e = first_expr("grep error log.txt 2>/dev/null | wc -l");
    if let Expression::Pipe { left, .. } = &e {
        if let Expression::Command(c) = left.as_ref() {
            assert_eq!(c.redirects.len(), 1);
            assert_eq!(c.redirects[0].fd, Some(2));
        } else {
            panic!("expected Command on left");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn and_or_chain() {
    let e = first_expr("mkdir -p dir && cd dir && echo done");
    if let Expression::And { left, .. } = &e {
        assert!(matches!(left.as_ref(), Expression::And { .. }));
    } else {
        panic!("expected And");
    }
}

#[test]
fn or_fallback() {
    let e = first_expr("cmd1 || cmd2 || cmd3");
    if let Expression::Or { left, .. } = &e {
        assert!(matches!(left.as_ref(), Expression::Or { .. }));
    } else {
        panic!("expected Or");
    }
}

#[test]
fn mixed_and_or_with_pipeline() {
    let e = first_expr("cmd1 | cmd2 && cmd3 | cmd4 || cmd5");
    if let Expression::Or { left, .. } = &e {
        if let Expression::And { left, right, .. } = left.as_ref() {
            assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
            assert!(matches!(right.as_ref(), Expression::Pipe { .. }));
        } else {
            panic!("expected And inside Or");
        }
    } else {
        panic!("expected Or");
    }
}

#[test]
fn complex_pipeline() {
    assert!(matches!(
        first_expr("ps aux | grep nginx | grep -v grep | awk '{print $2}'"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn line_continuation_in_pipeline() {
    assert!(matches!(
        first_expr("echo hello |\ngrep h |\nsort"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn compound_command_in_pipeline() {
    let e = first_expr("for i in a b; do echo $i; done | sort");
    if let Expression::Pipe { left, .. } = &e {
        assert!(matches!(
            left.as_ref(),
            Expression::Compound {
                body: CompoundCommand::ForClause { .. },
                ..
            }
        ));
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn subshell_pipeline() {
    let e = first_expr("(cd /tmp && ls) | grep test");
    if let Expression::Pipe { left, .. } = &e {
        assert!(matches!(
            left.as_ref(),
            Expression::Compound {
                body: CompoundCommand::Subshell { .. },
                ..
            }
        ));
    } else {
        panic!("expected Pipe");
    }
}

// --- backslash-newline line continuation ---

#[test]
fn backslash_newline_mid_word() {
    // \<newline> inside a word is line continuation — removed entirely
    let e = first_expr("ec\\\nho hello");
    if let Expression::Command(c) = &e {
        assert_eq!(word_literal(&c.arguments[0]), "echo");
    } else {
        panic!("expected Command");
    }
}

// --- backslash-newline line continuation before compound commands ---
// The lexer should consume `\<newline>` between tokens so that
// `cmd | \<newline>while ...` is equivalent to `cmd | while ...`.

#[test]
fn backslash_newline_pipe_into_while() {
    // Source: /usr/sbin/pam_namespace_helper, /usr/sbin/aa-remove-unknown
    let input = "echo |\n while read x; do echo \"$x\"; done";
    assert!(parse(input).is_ok(), "bare newline after pipe works");

    let input = "echo | \\\nwhile read x; do echo \"$x\"; done";
    assert!(
        parse(input).is_ok(),
        "backslash-newline after pipe should also work"
    );
}

#[test]
fn backslash_newline_pipe_into_for() {
    let input = "echo | \\\nfor x in a b; do echo \"$x\"; done";
    assert!(
        parse(input).is_ok(),
        "for loop after pipe with line continuation"
    );
}

#[test]
fn backslash_newline_pipe_into_if() {
    // Source: /usr/bin/ssh-copy-id
    let input = "printf '%s\\n' \"$x\" | \\\n  if [ \"$y\" ] ; then\n    echo sftp\n  else\n    echo ssh\n  fi";
    assert!(
        parse(input).is_ok(),
        "pipe into if with line continuation"
    );
}

#[test]
fn backslash_newline_and_then_brace_group() {
    // Source: /etc/cron.daily/man-db (if ... && \<newline> { ...; }; then)
    let input = "true && \\\n{ echo yes; }";
    assert!(
        parse(input).is_ok(),
        "brace group after && with line continuation"
    );
}

#[test]
fn backslash_newline_or_then_brace_group() {
    // Source: /usr/bin/savelog
    let input = "false || \\\n{\n  echo fallback\n}";
    assert!(
        parse(input).is_ok(),
        "brace group after || with line continuation"
    );
}

#[test]
fn backslash_newline_or_then_if() {
    // Source: /usr/lib/git-core/git-instaweb
    let input = "cmd1 || \\\nif test -f foo\nthen\n  echo yes\nfi";
    assert!(
        parse(input).is_ok(),
        "if command after || with line continuation"
    );
}
