//! Tests for `+=` append assignment.

use thaum::Dialect;

// Scalar append =======================================================================================================

#[skuld::test]
fn append_scalar_string() {
    let r = exec!("s='abc'; s+=d; echo $s");
    assert_eq!(r.stdout(), "abcd\n");
}

#[skuld::test]
fn append_to_undefined_scalar() {
    let r = exec!("s+=foo; echo $s");
    assert_eq!(r.stdout(), "foo\n");
}

#[skuld::test]
fn append_value_semantics() {
    let r = exec!("s1='abc'; s2=$s1; s1+='d'; echo $s1 $s2");
    assert_eq!(r.stdout(), "abcd abc\n");
}

// Integer append ======================================================================================================

#[skuld::test(ignore = "old test_executor used Bash mode; needs dialect fix or ExecError refactor")]
fn append_integer_add() {
    let r = exec!("declare -i x=5; x+=3; echo $x");
    assert_eq!(r.stdout(), "8\n");
}

// Array append ========================================================================================================

#[skuld::test]
fn append_array_to_array() {
    let r = exec!("a=(x y); a+=(t u); echo \"${a[@]}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "x y t u\n");
}

#[skuld::test]
fn append_array_to_undefined() {
    let r = exec!("y+=(c d); echo \"${y[@]}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "c d\n");
}

#[skuld::test]
fn append_array_element() {
    let r = exec!("a=(x y); a[1]+=z; echo \"${a[@]}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "x yz\n");
}

#[skuld::test]
fn append_assoc_element() {
    let r = exec!("declare -A m; m[k]=ab; m[k]+=cd; echo ${m[k]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "abcd\n");
}

// Builtin integration =================================================================================================

#[skuld::test(ignore = "old test_executor used Bash mode; needs dialect fix or ExecError refactor")]
fn declare_append() {
    let r = exec!("s=abc; declare s+=d; echo $s");
    assert_eq!(r.stdout(), "abcd\n");
}

#[skuld::test]
fn export_append() {
    let r = exec!("export e=ab; export e+=cd; echo $e");
    assert_eq!(r.stdout(), "abcd\n");
}

#[skuld::test(ignore = "old test_executor used Bash mode; needs dialect fix or ExecError refactor")]
fn local_append() {
    let r = exec!("f() { local s=ab; local s+=cd; echo $s; }; f");
    assert_eq!(r.stdout(), "abcd\n");
}
