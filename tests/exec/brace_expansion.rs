use crate::*;

skuld::default_labels!(exec);

// Simple list expansion ===============================================================================================

#[skuld::test]
fn simple_list() {
    let (out, status) = bash_exec_ok("echo {a,b,c}");
    assert_eq!(status, 0);
    assert_eq!(out, "a b c\n");
}

#[skuld::test]
fn list_with_prefix_suffix() {
    let (out, _) = bash_exec_ok("echo -{a,b,c}-");
    assert_eq!(out, "-a- -b- -c-\n");
}

#[skuld::test]
fn single_item_literal() {
    let (out, _) = bash_exec_ok("echo {foo}");
    assert_eq!(out, "{foo}\n");
}

// Cartesian product ===================================================================================================

#[skuld::test]
fn double_expansion() {
    let (out, _) = bash_exec_ok("echo {a,b}_{c,d}");
    assert_eq!(out, "a_c a_d b_c b_d\n");
}

#[skuld::test]
fn triple_expansion() {
    let (out, _) = bash_exec_ok("echo {0,1}{0,1}{0,1}");
    assert_eq!(out, "000 001 010 011 100 101 110 111\n");
}

// Nested braces =======================================================================================================

#[skuld::test]
fn nested() {
    let (out, _) = bash_exec_ok("echo -{A,={a,b}=,B}-");
    assert_eq!(out, "-A- -=a=- -=b=- -B-\n");
}

#[skuld::test]
fn triple_nested() {
    let (out, _) = bash_exec_ok("echo -{A,={a,.{x,y}.,b}=,B}-");
    assert_eq!(out, "-A- -=a=- -=.x.=- -=.y.=- -=b=- -B-\n");
}

// Empty alternatives ==================================================================================================

#[skuld::test]
fn empty_alternative() {
    let (out, _) = bash_exec_ok("echo a{X,,Y}b");
    assert_eq!(out, "aXb ab aYb\n");
}

// Numeric sequence ====================================================================================================

#[skuld::test]
fn numeric_range() {
    let (out, _) = bash_exec_ok("echo -{1..5}-");
    assert_eq!(out, "-1- -2- -3- -4- -5-\n");
}

#[skuld::test]
fn numeric_range_with_step() {
    let (out, _) = bash_exec_ok("echo -{1..8..3}-");
    assert_eq!(out, "-1- -4- -7-\n");
}

#[skuld::test]
fn numeric_range_with_step_exact() {
    let (out, _) = bash_exec_ok("echo -{1..10..3}-");
    assert_eq!(out, "-1- -4- -7- -10-\n");
}

#[skuld::test]
fn numeric_descending() {
    let (out, _) = bash_exec_ok("echo -{5..1}-");
    assert_eq!(out, "-5- -4- -3- -2- -1-\n");
}

#[skuld::test]
fn numeric_descending_with_negative_step() {
    let (out, _) = bash_exec_ok("echo -{8..1..-3}-");
    assert_eq!(out, "-8- -5- -2-\n");
}

// Zero-padding ========================================================================================================

#[skuld::test]
fn zero_padding() {
    let (out, _) = bash_exec_ok("echo -{01..03}-");
    assert_eq!(out, "-01- -02- -03-\n");
}

#[skuld::test]
fn zero_padding_cross_boundary() {
    let (out, _) = bash_exec_ok("echo -{09..12}-");
    assert_eq!(out, "-09- -10- -11- -12-\n");
}

#[skuld::test]
fn zero_padding_descending() {
    let (out, _) = bash_exec_ok("echo -{12..07}-");
    assert_eq!(out, "-12- -11- -10- -09- -08- -07-\n");
}

// Character sequence ==================================================================================================

#[skuld::test]
fn char_range() {
    let (out, _) = bash_exec_ok("echo -{a..e}-");
    assert_eq!(out, "-a- -b- -c- -d- -e-\n");
}

#[skuld::test]
fn char_range_with_step() {
    let (out, _) = bash_exec_ok("echo -{a..e..2}-");
    assert_eq!(out, "-a- -c- -e-\n");
}

#[skuld::test]
fn char_range_descending() {
    let (out, _) = bash_exec_ok("echo -{e..a}-");
    assert_eq!(out, "-e- -d- -c- -b- -a-\n");
}

// No expansion in assignment context ==================================================================================

#[skuld::test]
fn no_expansion_in_assignment() {
    let (out, _) = bash_exec_ok("v={X,Y}\necho $v");
    assert_eq!(out, "{X,Y}\n");
}

// Singleton ranges ====================================================================================================

#[skuld::test]
fn singleton_numeric() {
    let (out, _) = bash_exec_ok("echo {1..1}-");
    assert_eq!(out, "1-\n");
}

#[skuld::test]
fn singleton_negative() {
    let (out, _) = bash_exec_ok("echo {-9..-9}-");
    assert_eq!(out, "-9-\n");
}

#[skuld::test]
fn singleton_char() {
    let (out, _) = bash_exec_ok("echo {a..a}-");
    assert_eq!(out, "a-\n");
}

// Variables inside braces (requires parser fix) =======================================================================

#[skuld::test]
fn variable_in_braces() {
    let (out, _) = bash_exec_ok("a=A\necho -{$a,b}-");
    assert_eq!(out, "-A- -b-\n");
}

// Invalid sequences (literal fallback) ================================================================================

#[skuld::test]
fn invalid_no_comma_no_range() {
    let (out, _) = bash_exec_ok("echo {1.3}");
    assert_eq!(out, "{1.3}\n");
}

#[skuld::test]
fn invalid_triple_dot() {
    let (out, _) = bash_exec_ok("echo {1...3}");
    assert_eq!(out, "{1...3}\n");
}
