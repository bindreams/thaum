use crate::ast::{BraceExpansionKind, Fragment};

use super::brace_expansion::expand_braces;

skuld::default_labels!(exec);

/// Helper: extract literal strings from expansion results.
fn to_strings(result: &[Vec<Fragment>]) -> Vec<String> {
    result
        .iter()
        .map(|frags| {
            frags
                .iter()
                .map(|f| match f {
                    Fragment::Literal(s) => s.clone(),
                    other => panic!("expected Literal, got {other:?}"),
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

// List expansion ======================================================================================================

#[skuld::test]
fn list_simple() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
        vec![Fragment::Literal("a".into())],
        vec![Fragment::Literal("b".into())],
        vec![Fragment::Literal("c".into())],
    ]))];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["a", "b", "c"]);
}

#[skuld::test]
fn list_with_prefix_suffix() {
    let frags = vec![
        Fragment::Literal("-".into()),
        Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
            vec![Fragment::Literal("a".into())],
            vec![Fragment::Literal("b".into())],
        ])),
        Fragment::Literal("-".into()),
    ];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["-a-", "-b-"]);
}

#[skuld::test]
fn list_empty_items() {
    let frags = vec![
        Fragment::Literal("a".into()),
        Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
            vec![Fragment::Literal("X".into())],
            vec![],
            vec![Fragment::Literal("Y".into())],
        ])),
        Fragment::Literal("b".into()),
    ];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["aXb", "ab", "aYb"]);
}

// Cartesian product ===================================================================================================

#[skuld::test]
fn cartesian_product() {
    let frags = vec![
        Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
            vec![Fragment::Literal("a".into())],
            vec![Fragment::Literal("b".into())],
        ])),
        Fragment::Literal("_".into()),
        Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
            vec![Fragment::Literal("c".into())],
            vec![Fragment::Literal("d".into())],
        ])),
    ];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["a_c", "a_d", "b_c", "b_d"]);
}

#[skuld::test]
fn triple_cartesian() {
    let make_01 = || {
        Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
            vec![Fragment::Literal("0".into())],
            vec![Fragment::Literal("1".into())],
        ]))
    };
    let frags = vec![make_01(), make_01(), make_01()];
    let result = expand_braces(&frags);
    assert_eq!(
        to_strings(&result),
        vec!["000", "001", "010", "011", "100", "101", "110", "111"]
    );
}

// Nested expansion ====================================================================================================

#[skuld::test]
fn nested_list() {
    // {A,={a,b}=,B}
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
        vec![Fragment::Literal("A".into())],
        vec![
            Fragment::Literal("=".into()),
            Fragment::BashBraceExpansion(BraceExpansionKind::List(vec![
                vec![Fragment::Literal("a".into())],
                vec![Fragment::Literal("b".into())],
            ])),
            Fragment::Literal("=".into()),
        ],
        vec![Fragment::Literal("B".into())],
    ]))];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["A", "=a=", "=b=", "B"]);
}

// Sequence expansion ==================================================================================================

#[skuld::test]
fn sequence_numeric() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "1".into(),
        end: "5".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["1", "2", "3", "4", "5"]);
}

#[skuld::test]
fn sequence_descending() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "5".into(),
        end: "1".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["5", "4", "3", "2", "1"]);
}

#[skuld::test]
fn sequence_with_step() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "1".into(),
        end: "10".into(),
        step: Some("3".into()),
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["1", "4", "7", "10"]);
}

#[skuld::test]
fn sequence_descending_with_negative_step() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "8".into(),
        end: "1".into(),
        step: Some("-3".into()),
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["8", "5", "2"]);
}

#[skuld::test]
fn sequence_singleton() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "5".into(),
        end: "5".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["5"]);
}

#[skuld::test]
fn sequence_negative_singleton() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "-9".into(),
        end: "-9".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["-9"]);
}

// Zero-padding ========================================================================================================

#[skuld::test]
fn sequence_zero_padding() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "01".into(),
        end: "03".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["01", "02", "03"]);
}

#[skuld::test]
fn sequence_zero_padding_descending() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "12".into(),
        end: "07".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["12", "11", "10", "09", "08", "07"]);
}

#[skuld::test]
fn sequence_zero_padding_cross_boundary() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "09".into(),
        end: "12".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["09", "10", "11", "12"]);
}

// Character sequences =================================================================================================

#[skuld::test]
fn sequence_char() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "a".into(),
        end: "e".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["a", "b", "c", "d", "e"]);
}

#[skuld::test]
fn sequence_char_descending() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "e".into(),
        end: "a".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["e", "d", "c", "b", "a"]);
}

#[skuld::test]
fn sequence_char_with_step() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "a".into(),
        end: "e".into(),
        step: Some("2".into()),
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["a", "c", "e"]);
}

// Invalid sequences (literal fallback) ================================================================================

#[skuld::test]
fn sequence_invalid_step_zero() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "1".into(),
        end: "5".into(),
        step: Some("0".into()),
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["{1..5..0}"]);
}

#[skuld::test]
fn sequence_invalid_step_wrong_sign() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "1".into(),
        end: "5".into(),
        step: Some("-1".into()),
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["{1..5..-1}"]);
}

#[skuld::test]
fn sequence_invalid_non_numeric() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "foo".into(),
        end: "bar".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["{foo..bar}"]);
}

#[skuld::test]
fn sequence_invalid_mixed_case_chars() {
    let frags = vec![Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
        start: "a".into(),
        end: "Z".into(),
        step: None,
    })];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["{a..Z}"]);
}

// Passthrough =========================================================================================================

#[skuld::test]
fn no_brace_passthrough() {
    let frags = vec![Fragment::Literal("hello".into())];
    let result = expand_braces(&frags);
    assert_eq!(result.len(), 1);
    assert_eq!(to_strings(&result), vec!["hello"]);
}

#[skuld::test]
fn mixed_fragments_passthrough() {
    let frags = vec![
        Fragment::Literal("prefix".into()),
        Fragment::SingleQuoted("quoted".into()),
    ];
    let result = expand_braces(&frags);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].len(), 2);
}

// Sequence with prefix/suffix =========================================================================================

#[skuld::test]
fn sequence_with_prefix_suffix() {
    let frags = vec![
        Fragment::Literal("-".into()),
        Fragment::BashBraceExpansion(BraceExpansionKind::Sequence {
            start: "1".into(),
            end: "3".into(),
            step: None,
        }),
        Fragment::Literal("-".into()),
    ];
    let result = expand_braces(&frags);
    assert_eq!(to_strings(&result), vec!["-1-", "-2-", "-3-"]);
}
