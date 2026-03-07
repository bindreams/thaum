//! Tests for `+=` append assignment.

use crate::{bash_exec_ok, exec_ok};

// Scalar append =======================================================================================================

#[skuld::test]
fn append_scalar_string() {
    let (out, status) = exec_ok("s='abc'; s+=d; echo $s");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd\n");
}

#[skuld::test]
fn append_to_undefined_scalar() {
    let (out, status) = exec_ok("s+=foo; echo $s");
    assert_eq!(status, 0);
    assert_eq!(out, "foo\n");
}

#[skuld::test]
fn append_value_semantics() {
    let (out, status) = exec_ok("s1='abc'; s2=$s1; s1+='d'; echo $s1 $s2");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd abc\n");
}

// Integer append ======================================================================================================

#[skuld::test]
fn append_integer_add() {
    let (out, status) = exec_ok("declare -i x=5; x+=3; echo $x");
    assert_eq!(status, 0);
    assert_eq!(out, "8\n");
}

// Array append ========================================================================================================

#[skuld::test]
fn append_array_to_array() {
    let (out, status) = bash_exec_ok("a=(x y); a+=(t u); echo \"${a[@]}\"");
    assert_eq!(status, 0);
    assert_eq!(out, "x y t u\n");
}

#[skuld::test]
fn append_array_to_undefined() {
    let (out, status) = bash_exec_ok("y+=(c d); echo \"${y[@]}\"");
    assert_eq!(status, 0);
    assert_eq!(out, "c d\n");
}

#[skuld::test]
fn append_array_element() {
    let (out, status) = bash_exec_ok("a=(x y); a[1]+=z; echo \"${a[@]}\"");
    assert_eq!(status, 0);
    assert_eq!(out, "x yz\n");
}

#[skuld::test]
fn append_assoc_element() {
    let (out, status) = bash_exec_ok("declare -A m; m[k]=ab; m[k]+=cd; echo ${m[k]}");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd\n");
}

// Builtin integration =================================================================================================

#[skuld::test]
fn declare_append() {
    let (out, status) = exec_ok("s=abc; declare s+=d; echo $s");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd\n");
}

#[skuld::test]
fn export_append() {
    let (out, status) = exec_ok("export e=ab; export e+=cd; echo $e");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd\n");
}

#[skuld::test]
fn local_append() {
    let (out, status) = exec_ok("f() { local s=ab; local s+=cd; echo $s; }; f");
    assert_eq!(status, 0);
    assert_eq!(out, "abcd\n");
}
