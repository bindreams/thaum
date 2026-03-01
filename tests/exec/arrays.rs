use crate::*;

// Bash indexed arrays -------------------------------------------------------------------------------------------------

#[testutil::test]
fn array_literal_assignment() {
    let (out, status) = bash_exec_ok("a=(one two three); echo ${a[0]}");
    assert_eq!(status, 0);
    assert_eq!(out, "one\n");
}

#[testutil::test]
fn array_element_access() {
    let (out, _) = bash_exec_ok("a=(x y z); echo ${a[1]}");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn array_all_elements_at() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${a[@]}");
    assert_eq!(out, "a b c\n");
}

#[testutil::test]
fn array_all_elements_star() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${a[*]}");
    assert_eq!(out, "a b c\n");
}

#[testutil::test]
fn array_length() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${#a[@]}");
    assert_eq!(out, "3\n");
}

#[testutil::test]
fn array_element_length() {
    let (out, _) = bash_exec_ok("a=(hello); echo ${#a[0]}");
    assert_eq!(out, "5\n");
}

#[testutil::test]
fn array_default_index() {
    // $a is equivalent to ${a[0]} in bash
    let (out, _) = bash_exec_ok("a=(first second); echo $a");
    assert_eq!(out, "first\n");
}

#[testutil::test]
fn array_indexed_assignment() {
    let (out, _) = bash_exec_ok("a[0]=hello; echo ${a[0]}");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn array_sparse_assignment() {
    let (out, _) = bash_exec_ok("a[5]=five; echo ${a[5]}");
    assert_eq!(out, "five\n");
}

#[testutil::test]
fn array_overwrite_element() {
    let (out, _) = bash_exec_ok("a=(x y z); a[1]=Y; echo ${a[@]}");
    assert_eq!(out, "x Y z\n");
}

#[testutil::test]
fn array_unset_element() {
    let (out, _) = bash_exec_ok("a=(x y z); unset a[1]; echo ${a[@]}");
    assert_eq!(out, "x z\n");
}

#[testutil::test]
fn array_unset_whole() {
    let (out, _) = bash_exec_ok("a=(x y z); unset a; echo \"${a[@]}\"");
    assert_eq!(out, "\n");
}

#[testutil::test]
fn array_arith_access() {
    let (out, _) = bash_exec_ok("a=(10 20 30); echo $(( a[1] + a[2] ))");
    assert_eq!(out, "50\n");
}

#[testutil::test]
fn array_arith_assign() {
    let (out, _) = bash_exec_ok("(( a[0] = 42 )); echo ${a[0]}");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn array_for_loop() {
    let (out, _) = bash_exec_ok(r#"a=(x y z); for i in ${a[@]}; do echo $i; done"#);
    assert_eq!(out, "x\ny\nz\n");
}

// Associative arrays --------------------------------------------------------------------------------------------------

#[testutil::test]
fn assoc_array_basic() {
    let (out, _) = bash_exec_ok("declare -A m; m[foo]=bar; echo ${m[foo]}");
    assert_eq!(out, "bar\n");
}

#[testutil::test]
fn assoc_array_all_elements() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; m[b]=2; echo ${#m[@]}");
    assert_eq!(out, "2\n");
}

#[testutil::test]
fn assoc_array_overwrite() {
    let (out, _) = bash_exec_ok("declare -A m; m[k]=old; m[k]=new; echo ${m[k]}");
    assert_eq!(out, "new\n");
}

#[testutil::test]
fn assoc_array_unset_element() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; m[b]=2; unset m[a]; echo ${#m[@]}");
    assert_eq!(out, "1\n");
}

#[testutil::test]
fn assoc_array_unset_whole() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; unset m; echo \"${m[@]}\"");
    assert_eq!(out, "\n");
}

// typeset/declare + flags (attribute removal) -------------------------------------------------------------------------

#[testutil::test]
fn typeset_plus_r_bash_silently_fails() {
    // Bash behavior: typeset +r does NOT remove readonly
    let (out, _) = bash_exec_ok("readonly x=1; typeset +r x 2>/dev/null; echo $x");
    assert_eq!(out, "1\n");
}

#[testutil::test]
fn typeset_plus_x_unexports() {
    // +x removes export attribute, value preserved
    let (out, _) = bash_exec_ok("export x=hello; declare +x x; echo $x");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn typeset_plus_i_removes_integer() {
    // +i removes integer attribute — subsequent assignment stores string
    let (out, _) = bash_exec_ok("declare -i x=42; declare +i x; x=hello; echo $x");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn typeset_plus_l_removes_lowercase() {
    // +l removes lowercase attribute — subsequent assignment preserves case
    let (out, _) = bash_exec_ok("declare -l x=hello; declare +l x; x=WORLD; echo $x");
    assert_eq!(out, "WORLD\n");
}

#[testutil::test]
fn typeset_plus_u_removes_uppercase() {
    let (out, _) = bash_exec_ok("declare -u x=HELLO; declare +u x; x=world; echo $x");
    assert_eq!(out, "world\n");
}

// declare/typeset builtin ---------------------------------------------------------------------------------------------

#[testutil::test]
fn declare_indexed_array() {
    // NOTE: `declare -a a=(1 2 3)` is not yet supported because the parser
    // does not handle compound array assignment in argument position.
    // Use separate assignment instead.
    let (out, _) = bash_exec_ok("declare -a a; a=(1 2 3); echo ${a[1]}");
    assert_eq!(out, "2\n");
}

#[testutil::test]
fn declare_assoc_array_inline() {
    let (out, _) = bash_exec_ok("declare -A m=([foo]=1 [bar]=2); echo ${m[foo]} ${m[bar]}");
    assert_eq!(out, "1 2\n");
}

#[testutil::test]
fn declare_readonly() {
    let status = bash_exec_result("declare -r x=42; x=99");
    assert_ne!(status, 0);
}

#[testutil::test]
fn declare_export() {
    let (out, _) = bash_exec_ok("declare -x MYVAR=hello; echo $MYVAR");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn declare_integer() {
    let (out, _) = bash_exec_ok("declare -i x; x='2+3'; echo $x");
    assert_eq!(out, "5\n");
}

#[testutil::test]
fn declare_integer_assign() {
    let (out, _) = bash_exec_ok("declare -i x=10; x='x+5'; echo $x");
    assert_eq!(out, "15\n");
}

#[testutil::test]
fn declare_integer_inline_arithmetic() {
    // declare -i x=2+3 should evaluate the arithmetic in the declare itself.
    let (out, _) = bash_exec_ok("declare -i x=2+3; echo $x");
    assert_eq!(out, "5\n");
}

#[testutil::test]
fn declare_plus_i_removes_arithmetic() {
    // declare +i removes the integer attribute; assignment should be literal.
    let (out, _) = bash_exec_ok("declare -i x=10; declare +i x=2+3; echo $x");
    assert_eq!(out, "2+3\n");
}

#[testutil::test]
fn declare_integer_with_variable_ref() {
    // declare -i y=x+5 should resolve x in arithmetic context.
    let (out, _) = bash_exec_ok("declare -i x=10; declare -i y=x+5; echo $y");
    assert_eq!(out, "15\n");
}

#[testutil::test]
fn declare_local_in_function() {
    let (out, _) = bash_exec_ok("f() { declare x=inner; echo $x; }; x=outer; f; echo $x");
    assert_eq!(out, "inner\nouter\n");
}

#[testutil::test]
fn declare_global_in_function() {
    let (out, _) = bash_exec_ok("f() { declare -g x=global; }; f; echo $x");
    assert_eq!(out, "global\n");
}

#[testutil::test]
fn typeset_is_synonym() {
    let (out, _) = bash_exec_ok("typeset -i x=5; echo $x");
    assert_eq!(out, "5\n");
}

#[testutil::test]
fn declare_print_scalar() {
    let (out, _) = bash_exec_ok("x=hello; declare -p x");
    assert!(out.contains("declare") && out.contains("x=") && out.contains("hello"));
}

#[testutil::test]
fn declare_lowercase() {
    let (out, _) = bash_exec_ok("declare -l x; x=HELLO; echo $x");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn declare_uppercase() {
    let (out, _) = bash_exec_ok("declare -u x; x=hello; echo $x");
    assert_eq!(out, "HELLO\n");
}

// Nameref (declare -n) ------------------------------------------------------------------------------------------------

#[testutil::test]
fn nameref_basic() {
    let (out, _) = bash_exec_ok("declare -n r=x; x=hello; echo $r");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn nameref_write() {
    let (out, _) = bash_exec_ok("declare -n r=x; r=world; echo $x");
    assert_eq!(out, "world\n");
}

#[testutil::test]
fn nameref_function_param() {
    let (out, _) = bash_exec_ok("f() { declare -n out=$1; out=42; }; f result; echo $result");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn nameref_chain() {
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=c; c=deep; echo $a");
    assert_eq!(out, "deep\n");
}

#[testutil::test]
fn nameref_cycle() {
    // Cycle detection — should not infinite loop. ${a:-safe} provides fallback.
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}

#[testutil::test]
fn nameref_unset_target() {
    // unset through nameref unsets the target, not the ref
    let (out, _) = bash_exec_ok("declare -n r=x; x=hi; unset r; echo ${x:-gone}");
    assert_eq!(out, "gone\n");
}

#[testutil::test]
fn nameref_array() {
    let (out, _) = bash_exec_ok("a=(1 2 3); declare -n r=a; echo ${r[1]}");
    assert_eq!(out, "2\n");
}

#[testutil::test]
fn nameref_cycle_3way() {
    // 3-way cycle: a→b→c→a. Must not hang.
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=c; declare -n c=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}

#[testutil::test]
fn nameref_cycle_non_origin() {
    // x→a→b→a — x is not in the cycle, but the chain it enters is cyclic.
    // Must not hang. x resolves to a (or b), which is unset → fallback.
    let (out, _) = bash_exec_ok("declare -n x=a; declare -n a=b; declare -n b=a; echo ${x:-safe}");
    assert_eq!(out, "safe\n");
}

#[testutil::test]
fn nameref_cycle_write() {
    // Writing through a cycle must not hang — should fail gracefully.
    let status = bash_exec_ok("declare -n a=b; declare -n b=a; a=oops 2>/dev/null; echo ok").1;
    assert_eq!(status, 0); // shell survives, doesn't hang
}

#[testutil::test]
fn nameref_self_reference() {
    // declare -n a=a — self-referencing nameref. Must not hang.
    let (out, _) = bash_exec_ok("declare -n a=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}
