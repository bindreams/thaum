use crate::common::labels::EXEC;
use crate::*;

skuld::default_labels!(EXEC);

// Simple list expansion ===============================================================================================

#[skuld::test]
fn simple_list() {
    let r = exec!("echo {a,b,c}", dialect = Dialect::Bash);
    assert_eq!(r.status(), 0);
    assert_eq!(r.stdout(), "a b c\n");
}

#[skuld::test]
fn list_with_prefix_suffix() {
    let r = exec!("echo -{a,b,c}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-a- -b- -c-\n");
}

#[skuld::test]
fn single_item_literal() {
    let r = exec!("echo {foo}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "{foo}\n");
}

// Cartesian product ===================================================================================================

#[skuld::test]
fn double_expansion() {
    let r = exec!("echo {a,b}_{c,d}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "a_c a_d b_c b_d\n");
}

#[skuld::test]
fn triple_expansion() {
    let r = exec!("echo {0,1}{0,1}{0,1}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "000 001 010 011 100 101 110 111\n");
}

// Nested braces =======================================================================================================

#[skuld::test]
fn nested() {
    let r = exec!("echo -{A,={a,b}=,B}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-A- -=a=- -=b=- -B-\n");
}

#[skuld::test]
fn triple_nested() {
    let r = exec!("echo -{A,={a,.{x,y}.,b}=,B}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-A- -=a=- -=.x.=- -=.y.=- -=b=- -B-\n");
}

// Empty alternatives ==================================================================================================

#[skuld::test]
fn empty_alternative() {
    let r = exec!("echo a{X,,Y}b", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "aXb ab aYb\n");
}

// Numeric sequence ====================================================================================================

#[skuld::test]
fn numeric_range() {
    let r = exec!("echo -{1..5}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-1- -2- -3- -4- -5-\n");
}

#[skuld::test]
fn numeric_range_with_step() {
    let r = exec!("echo -{1..8..3}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-1- -4- -7-\n");
}

#[skuld::test]
fn numeric_range_with_step_exact() {
    let r = exec!("echo -{1..10..3}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-1- -4- -7- -10-\n");
}

#[skuld::test]
fn numeric_descending() {
    let r = exec!("echo -{5..1}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-5- -4- -3- -2- -1-\n");
}

#[skuld::test]
fn numeric_descending_with_negative_step() {
    let r = exec!("echo -{8..1..-3}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-8- -5- -2-\n");
}

// Zero-padding ========================================================================================================

#[skuld::test]
fn zero_padding() {
    let r = exec!("echo -{01..03}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-01- -02- -03-\n");
}

#[skuld::test]
fn zero_padding_cross_boundary() {
    let r = exec!("echo -{09..12}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-09- -10- -11- -12-\n");
}

#[skuld::test]
fn zero_padding_descending() {
    let r = exec!("echo -{12..07}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-12- -11- -10- -09- -08- -07-\n");
}

// Character sequence ==================================================================================================

#[skuld::test]
fn char_range() {
    let r = exec!("echo -{a..e}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-a- -b- -c- -d- -e-\n");
}

#[skuld::test]
fn char_range_with_step() {
    let r = exec!("echo -{a..e..2}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-a- -c- -e-\n");
}

#[skuld::test]
fn char_range_descending() {
    let r = exec!("echo -{e..a}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-e- -d- -c- -b- -a-\n");
}

// No expansion in assignment context ==================================================================================

#[skuld::test]
fn no_expansion_in_assignment() {
    let r = exec!("v={X,Y}\necho $v", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "{X,Y}\n");
}

// Singleton ranges ====================================================================================================

#[skuld::test]
fn singleton_numeric() {
    let r = exec!("echo {1..1}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "1-\n");
}

#[skuld::test]
fn singleton_negative() {
    let r = exec!("echo {-9..-9}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-9-\n");
}

#[skuld::test]
fn singleton_char() {
    let r = exec!("echo {a..a}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "a-\n");
}

// Variables inside braces (requires parser fix) =======================================================================

#[skuld::test]
fn variable_in_braces() {
    let r = exec!("a=A\necho -{$a,b}-", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "-A- -b-\n");
}

// Invalid sequences (literal fallback) ================================================================================

#[skuld::test]
fn invalid_no_comma_no_range() {
    let r = exec!("echo {1.3}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "{1.3}\n");
}

#[skuld::test]
fn invalid_triple_dot() {
    let r = exec!("echo {1...3}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "{1...3}\n");
}
