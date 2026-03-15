use thaum::Dialect;

// Bash indexed arrays -------------------------------------------------------------------------------------------------

#[skuld::test]
fn array_literal_assignment() {
    let r = exec!("a=(one two three); echo ${a[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "one\n");
}

#[skuld::test]
fn array_element_access() {
    let r = exec!("a=(x y z); echo ${a[1]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn array_all_elements_at() {
    let r = exec!("a=(a b c); echo ${a[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "a b c\n");
}

#[skuld::test]
fn array_all_elements_star() {
    let r = exec!("a=(a b c); echo ${a[*]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "a b c\n");
}

#[skuld::test]
fn array_length() {
    let r = exec!("a=(a b c); echo ${#a[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "3\n");
}

#[skuld::test]
fn array_element_length() {
    let r = exec!("a=(hello); echo ${#a[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "5\n");
}

#[skuld::test]
fn array_default_index() {
    // $a is equivalent to ${a[0]} in bash
    let r = exec!("a=(first second); echo $a", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "first\n");
}

#[skuld::test]
fn array_indexed_assignment() {
    let r = exec!("a[0]=hello; echo ${a[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn array_sparse_assignment() {
    let r = exec!("a[5]=five; echo ${a[5]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "five\n");
}

#[skuld::test]
fn array_subscript_arithmetic() {
    // Array subscripts should be evaluated as arithmetic expressions.
    let r = exec!("a[1+1]=hello; echo ${a[2]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn array_subscript_variable() {
    // Variables in subscripts should be resolved in arithmetic context.
    let r = exec!("i=3; a[$i]=world; echo ${a[3]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "world\n");
}

#[skuld::test]
fn array_overwrite_element() {
    let r = exec!("a=(x y z); a[1]=Y; echo ${a[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "x Y z\n");
}

#[skuld::test]
fn array_unset_element() {
    let r = exec!("a=(x y z); unset a[1]; echo ${a[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "x z\n");
}

#[skuld::test]
fn array_unset_whole() {
    let r = exec!("a=(x y z); unset a; echo \"${a[@]}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn array_arith_access() {
    let r = exec!("a=(10 20 30); echo $(( a[1] + a[2] ))", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "50\n");
}

#[skuld::test]
fn array_arith_assign() {
    let r = exec!("(( a[0] = 42 )); echo ${a[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn array_for_loop() {
    let r = exec!(
        r#"a=(x y z); for i in ${a[@]}; do echo $i; done"#,
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "x\ny\nz\n");
}

// Associative arrays --------------------------------------------------------------------------------------------------

#[skuld::test]
fn assoc_array_basic() {
    let r = exec!("declare -A m; m[foo]=bar; echo ${m[foo]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "bar\n");
}

#[skuld::test]
fn assoc_array_all_elements() {
    let r = exec!("declare -A m; m[a]=1; m[b]=2; echo ${#m[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "2\n");
}

#[skuld::test]
fn assoc_array_overwrite() {
    let r = exec!(
        "declare -A m; m[k]=old; m[k]=new; echo ${m[k]}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "new\n");
}

#[skuld::test]
fn assoc_array_unset_element() {
    let r = exec!(
        "declare -A m; m[a]=1; m[b]=2; unset m[a]; echo ${#m[@]}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "1\n");
}

#[skuld::test]
fn assoc_array_unset_whole() {
    let r = exec!(
        "declare -A m; m[a]=1; unset m; echo \"${m[@]}\"",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "\n");
}

// typeset/declare + flags (attribute removal) -------------------------------------------------------------------------

#[skuld::test]
fn typeset_plus_r_bash_silently_fails() {
    // Bash behavior: typeset +r does NOT remove readonly
    let r = exec!(
        "readonly x=1; typeset +r x 2>/dev/null; echo $x",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "1\n");
}

#[skuld::test]
fn typeset_plus_x_unexports() {
    // +x removes export attribute, value preserved
    let r = exec!("export x=hello; declare +x x; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn typeset_plus_i_removes_integer() {
    // +i removes integer attribute — subsequent assignment stores string
    let r = exec!(
        "declare -i x=42; declare +i x; x=hello; echo $x",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn typeset_plus_l_removes_lowercase() {
    // +l removes lowercase attribute — subsequent assignment preserves case
    let r = exec!(
        "declare -l x=hello; declare +l x; x=WORLD; echo $x",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "WORLD\n");
}

#[skuld::test]
fn typeset_plus_u_removes_uppercase() {
    let r = exec!(
        "declare -u x=HELLO; declare +u x; x=world; echo $x",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "world\n");
}

// declare/typeset builtin ---------------------------------------------------------------------------------------------

#[skuld::test]
fn declare_indexed_array() {
    // NOTE: `declare -a a=(1 2 3)` is not yet supported because the parser
    // does not handle compound array assignment in argument position.
    // Use separate assignment instead.
    let r = exec!("declare -a a; a=(1 2 3); echo ${a[1]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "2\n");
}

#[skuld::test]
fn declare_assoc_array_inline() {
    let r = exec!(
        "declare -A m=([foo]=1 [bar]=2); echo ${m[foo]} ${m[bar]}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "1 2\n");
}

#[skuld::test]
fn declare_readonly() {
    let r = exec!("declare -r x=42; x=99", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0);
    assert!(r.stderr().contains("readonly variable"));
}

#[skuld::test]
fn declare_export() {
    let r = exec!("declare -x MYVAR=hello; echo $MYVAR", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn declare_integer() {
    let r = exec!("declare -i x; x='2+3'; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "5\n");
}

#[skuld::test]
fn declare_integer_assign() {
    let r = exec!("declare -i x=10; x='x+5'; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "15\n");
}

#[skuld::test]
fn declare_integer_inline_arithmetic() {
    // declare -i x=2+3 should evaluate the arithmetic in the declare itself.
    let r = exec!("declare -i x=2+3; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "5\n");
}

#[skuld::test]
fn declare_plus_i_removes_arithmetic() {
    // declare +i removes the integer attribute; assignment should be literal.
    let r = exec!("declare -i x=10; declare +i x=2+3; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "2+3\n");
}

#[skuld::test]
fn declare_integer_with_variable_ref() {
    // declare -i y=x+5 should resolve x in arithmetic context.
    let r = exec!("declare -i x=10; declare -i y=x+5; echo $y", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "15\n");
}

#[skuld::test]
fn declare_local_in_function() {
    let r = exec!(
        "f() { declare x=inner; echo $x; }; x=outer; f; echo $x",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "inner\nouter\n");
}

#[skuld::test]
fn declare_global_in_function() {
    let r = exec!("f() { declare -g x=global; }; f; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "global\n");
}

#[skuld::test]
fn typeset_is_synonym() {
    let r = exec!("typeset -i x=5; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "5\n");
}

#[skuld::test]
fn declare_print_scalar() {
    let r = exec!("x=hello; declare -p x", dialect = Dialect::Bash);
    let out = r.stdout();
    assert!(out.contains("declare") && out.contains("x=") && out.contains("hello"));
}

#[skuld::test]
fn declare_lowercase() {
    let r = exec!("declare -l x; x=HELLO; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn declare_uppercase() {
    let r = exec!("declare -u x; x=hello; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "HELLO\n");
}

// Nameref (declare -n) ------------------------------------------------------------------------------------------------

#[skuld::test]
fn nameref_basic() {
    let r = exec!("declare -n r=x; x=hello; echo $r", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn nameref_write() {
    let r = exec!("declare -n r=x; r=world; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "world\n");
}

#[skuld::test]
fn nameref_function_param() {
    let r = exec!(
        "f() { declare -n out=$1; out=42; }; f result; echo $result",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn nameref_chain() {
    let r = exec!(
        "declare -n a=b; declare -n b=c; c=deep; echo $a",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "deep\n");
}

#[skuld::test]
fn nameref_cycle() {
    // Cycle detection — should not infinite loop. ${a:-safe} provides fallback.
    let r = exec!(
        "declare -n a=b; declare -n b=a; echo ${a:-safe}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "safe\n");
}

#[skuld::test]
fn nameref_unset_target() {
    // unset through nameref unsets the target, not the ref
    let r = exec!(
        "declare -n r=x; x=hi; unset r; echo ${x:-gone}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "gone\n");
}

#[skuld::test]
fn nameref_array() {
    let r = exec!("a=(1 2 3); declare -n r=a; echo ${r[1]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "2\n");
}

#[skuld::test]
fn nameref_cycle_3way() {
    // 3-way cycle: a→b→c→a. Must not hang.
    let r = exec!(
        "declare -n a=b; declare -n b=c; declare -n c=a; echo ${a:-safe}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "safe\n");
}

#[skuld::test]
fn nameref_cycle_non_origin() {
    // x→a→b→a — x is not in the cycle, but the chain it enters is cyclic.
    // Must not hang. x resolves to a (or b), which is unset → fallback.
    let r = exec!(
        "declare -n x=a; declare -n a=b; declare -n b=a; echo ${x:-safe}",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "safe\n");
}

#[skuld::test]
fn nameref_cycle_write() {
    // Writing through a cycle must not hang — should fail gracefully.
    let r = exec!(
        "declare -n a=b; declare -n b=a; a=oops 2>/dev/null; echo ok",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "ok\n");
}

#[skuld::test]
fn nameref_self_reference() {
    // declare -n a=a — self-referencing nameref. Must not hang.
    let r = exec!("declare -n a=a; echo ${a:-safe}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "safe\n");
}
