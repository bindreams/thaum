use super::*;
use crate::span::Span;

fn span() -> Span {
    Span::new(0, 0)
}

fn literal(s: &str) -> Fragment {
    Fragment::Literal(s.into())
}

fn word(parts: Vec<Fragment>) -> Word {
    Word { parts, span: span() }
}

// Fragment ------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn literal_is_static() {
    assert_eq!(literal("hello").try_to_static_string(), Some("hello".into()));
}

#[testutil::test]
fn single_quoted_is_static() {
    assert_eq!(
        Fragment::SingleQuoted("world".into()).try_to_static_string(),
        Some("world".into())
    );
}

#[testutil::test]
fn ansi_c_quoted_is_static() {
    assert_eq!(
        Fragment::BashAnsiCQuoted("line\n".into()).try_to_static_string(),
        Some("line\n".into())
    );
}

#[testutil::test]
fn double_quoted_all_literal_is_static() {
    let frag = Fragment::DoubleQuoted(vec![literal("a"), literal("b")]);
    assert_eq!(frag.try_to_static_string(), Some("ab".into()));
}

#[testutil::test]
fn double_quoted_with_param_is_none() {
    let frag = Fragment::DoubleQuoted(vec![
        literal("hi "),
        Fragment::Parameter(ParameterExpansion::Simple("USER".into())),
    ]);
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn parameter_is_none() {
    let frag = Fragment::Parameter(ParameterExpansion::Simple("x".into()));
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn command_sub_is_none() {
    let frag = Fragment::CommandSubstitution(vec![]);
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn glob_is_none() {
    assert_eq!(Fragment::Glob(GlobChar::Star).try_to_static_string(), None);
}

#[testutil::test]
fn tilde_is_none() {
    assert_eq!(Fragment::TildePrefix("".into()).try_to_static_string(), None);
}

#[testutil::test]
fn arithmetic_is_none() {
    let frag = Fragment::ArithmeticExpansion(ArithExpr::Number(42));
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn brace_expansion_is_none() {
    let frag = Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "1".into(),
        end: "5".into(),
        step: None,
    });
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn extglob_is_none() {
    let frag = Fragment::BashExtGlob {
        kind: ExtGlobKind::ZeroOrMore,
        pattern: "*.rs".into(),
    };
    assert_eq!(frag.try_to_static_string(), None);
}

#[testutil::test]
fn locale_quoted_is_none() {
    let frag = Fragment::BashLocaleQuoted {
        raw: "hi".into(),
        parts: vec![literal("hi")],
    };
    assert_eq!(frag.try_to_static_string(), None);
}

// Word ----------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn word_single_literal() {
    assert_eq!(word(vec![literal("echo")]).try_to_static_string(), Some("echo".into()));
}

#[testutil::test]
fn word_concatenated_static() {
    let w = word(vec![literal("hel"), Fragment::SingleQuoted("lo".into())]);
    assert_eq!(w.try_to_static_string(), Some("hello".into()));
}

#[testutil::test]
fn word_with_dynamic_part() {
    let w = word(vec![
        literal("dir/"),
        Fragment::Parameter(ParameterExpansion::Simple("name".into())),
    ]);
    assert_eq!(w.try_to_static_string(), None);
}

#[testutil::test]
fn empty_word() {
    assert_eq!(word(vec![]).try_to_static_string(), Some(String::new()));
}

// Argument ------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn argument_word_delegates() {
    let arg = Argument::Word(word(vec![literal("test")]));
    assert_eq!(arg.try_to_static_string(), Some("test".into()));
}

#[testutil::test]
fn argument_atom_is_none() {
    let arg = Argument::Atom(Atom::BashProcessSubstitution {
        direction: ProcessDirection::In,
        body: vec![],
        span: span(),
    });
    assert_eq!(arg.try_to_static_string(), None);
}
