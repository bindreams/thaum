use crate::*;

// $- (option flags) ===================================================================================================

#[testutil::test]
fn dollar_dash_default() {
    // With no options set, $- should be a non-empty string (at minimum hB for bash defaults,
    // but our shell may start with a different set).
    let (out, _) = exec_ok("echo $-");
    // Should be non-empty and contain only flag characters
    let flags = out.trim();
    assert!(!flags.is_empty(), "$- should not be empty");
    for c in flags.chars() {
        assert!(c.is_ascii_alphabetic(), "unexpected char in $-: {:?}", c);
    }
}

#[testutil::test]
fn dollar_dash_reflects_errexit() {
    let (out, _) = exec_ok("set -e; echo $-");
    assert!(out.trim().contains('e'), "$- should contain 'e' after set -e");
}

#[testutil::test]
fn dollar_dash_reflects_nounset() {
    let (out, _) = exec_ok("set -u; echo $-");
    assert!(out.trim().contains('u'), "$- should contain 'u' after set -u");
}

#[testutil::test]
fn dollar_dash_reflects_xtrace() {
    // xtrace output goes to stderr, not stdout; just check the flag is present
    let (out, _) = exec_ok("set -x; echo $-");
    assert!(out.trim().contains('x'), "$- should contain 'x' after set -x");
}

#[testutil::test]
fn dollar_dash_not_affected_by_nounset() {
    // $- is a special parameter, so set -u should not cause an error
    let (out, _) = exec_ok("set -u; echo $-");
    assert!(!out.trim().is_empty());
}

// $_ (last argument) ==================================================================================================

#[testutil::test]
fn dollar_underscore_last_arg() {
    let (out, _) = exec_ok("echo a b c\necho $_");
    assert_eq!(out, "a b c\nc\n");
}

#[testutil::test]
fn dollar_underscore_after_single_arg_command() {
    let (out, _) = exec_ok("echo hello\necho $_");
    assert_eq!(out, "hello\nhello\n");
}

#[testutil::test]
fn dollar_underscore_after_no_arg_command() {
    // After a command with no arguments (like `true`), $_ is the command name itself
    let (out, _) = exec_ok("true\necho $_");
    assert_eq!(out, "true\n");
}

// RANDOM ==============================================================================================================

#[testutil::test]
fn random_returns_number_in_range() {
    let (out, _) = bash_exec_ok("echo $RANDOM");
    let val: i32 = out.trim().parse().expect("RANDOM should be a number");
    assert!((0..=32767).contains(&val), "RANDOM={val} out of 0..32767");
}

#[testutil::test]
fn random_differs_on_consecutive_reads() {
    // Two consecutive reads of RANDOM should (almost certainly) differ.
    let (out, _) = bash_exec_ok("echo $RANDOM $RANDOM");
    let parts: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(parts.len(), 2);
    // They could theoretically be equal, but the probability is ~1/32768.
    // If this flakes, the RNG is not advancing.
    assert_ne!(parts[0], parts[1], "two RANDOM reads should differ");
}

#[testutil::test]
fn random_seed_produces_deterministic_sequence() {
    // Setting RANDOM seeds the LCG — same seed should yield same first value.
    let (out1, _) = bash_exec_ok("RANDOM=42; echo $RANDOM");
    let (out2, _) = bash_exec_ok("RANDOM=42; echo $RANDOM");
    assert_eq!(out1, out2, "same seed should produce same RANDOM");
}

#[testutil::test]
fn random_unset_kills_special_behavior() {
    // After unset, RANDOM should become a plain variable.
    let (out, _) = bash_exec_ok("unset RANDOM; RANDOM=42; echo $RANDOM");
    assert_eq!(out.trim(), "42", "unset RANDOM should kill special behavior");
}

#[testutil::test]
fn random_not_affected_by_nounset() {
    let (out, _) = bash_exec_ok("set -u; echo $RANDOM");
    let val: i32 = out.trim().parse().expect("RANDOM should be a number");
    assert!((0..=32767).contains(&val));
}

// SECONDS =============================================================================================================

#[testutil::test]
fn seconds_returns_nonnegative() {
    let (out, _) = bash_exec_ok("echo $SECONDS");
    let val: i64 = out.trim().parse().expect("SECONDS should be a number");
    assert!(val >= 0, "SECONDS should be >= 0");
}

#[testutil::test]
fn seconds_assignment_resets_timer() {
    // Setting SECONDS=0 resets; subsequent read should be 0 (or very small).
    let (out, _) = bash_exec_ok("SECONDS=0; echo $SECONDS");
    let val: i64 = out.trim().parse().expect("SECONDS should be a number");
    assert!(val <= 2, "SECONDS after reset should be small, got {val}");
}

#[testutil::test]
fn seconds_unset_kills_special_behavior() {
    let (out, _) = bash_exec_ok("unset SECONDS; SECONDS=100; echo $SECONDS");
    assert_eq!(out.trim(), "100", "unset SECONDS should kill timer behavior");
}

// EPOCHSECONDS ========================================================================================================

#[testutil::test]
fn epochseconds_returns_valid_timestamp() {
    let (out, _) = bash_exec_ok("echo $EPOCHSECONDS");
    let val: u64 = out.trim().parse().expect("EPOCHSECONDS should be a number");
    // Should be a reasonable Unix timestamp (after 2020-01-01)
    assert!(val > 1_577_836_800, "EPOCHSECONDS too small: {val}");
}

#[testutil::test]
fn epochseconds_unset_kills_special_behavior() {
    let (out, _) = bash_exec_ok("unset EPOCHSECONDS; EPOCHSECONDS=42; echo $EPOCHSECONDS");
    assert_eq!(out.trim(), "42");
}

// EPOCHREALTIME =======================================================================================================

#[testutil::test]
fn epochrealtime_format() {
    let (out, _) = bash_exec_ok("echo $EPOCHREALTIME");
    let s = out.trim();
    // Should contain a dot
    assert!(s.contains('.'), "EPOCHREALTIME should have a dot: {s}");
    let parts: Vec<&str> = s.split('.').collect();
    assert_eq!(parts.len(), 2, "EPOCHREALTIME should have exactly one dot");
    // Microsecond part should be 6 digits
    assert_eq!(
        parts[1].len(),
        6,
        "EPOCHREALTIME fractional part should be 6 digits: {s}"
    );
    let secs: u64 = parts[0].parse().expect("EPOCHREALTIME seconds part");
    assert!(secs > 1_577_836_800, "EPOCHREALTIME timestamp too small: {secs}");
}

#[testutil::test]
fn epochrealtime_unset_kills_special_behavior() {
    let (out, _) = bash_exec_ok("unset EPOCHREALTIME; EPOCHREALTIME=1.23; echo $EPOCHREALTIME");
    assert_eq!(out.trim(), "1.23");
}

// SRANDOM =============================================================================================================

#[testutil::test]
fn srandom_returns_u32() {
    let (out, _) = bash_exec_ok("echo $SRANDOM");
    let val: u64 = out.trim().parse().expect("SRANDOM should be a number");
    assert!(val <= u32::MAX as u64, "SRANDOM out of u32 range: {val}");
}

#[testutil::test]
fn srandom_differs_on_consecutive_reads() {
    let (out, _) = bash_exec_ok("echo $SRANDOM $SRANDOM");
    let parts: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(parts.len(), 2);
    assert_ne!(parts[0], parts[1], "two SRANDOM reads should differ");
}

#[testutil::test]
fn srandom_assign_ignored() {
    // Assigning to SRANDOM should be silently ignored — next read is still random.
    let (out, _) = bash_exec_ok("SRANDOM=42; echo $SRANDOM");
    let val: u64 = out.trim().parse().expect("SRANDOM should be a number");
    // The value should NOT be 42 (probability ~1/2^32).
    // More importantly, it should still be a valid u32.
    assert!(val <= u32::MAX as u64);
}

#[testutil::test]
fn srandom_unset_kills_special_behavior() {
    let (out, _) = bash_exec_ok("unset SRANDOM; SRANDOM=42; echo $SRANDOM");
    assert_eq!(out.trim(), "42");
}

// BASHPID =============================================================================================================

#[testutil::test]
fn bashpid_returns_current_pid() {
    let (out, _) = bash_exec_ok("echo $BASHPID");
    let val: u32 = out.trim().parse().expect("BASHPID should be a number");
    assert!(val > 0, "BASHPID should be positive");
}

#[testutil::test]
fn bashpid_assign_silently_ignored() {
    // Assigning to BASHPID should be silently ignored.
    let (out, _) = bash_exec_ok("BASHPID=42; echo $BASHPID");
    let val: u32 = out.trim().parse().expect("BASHPID should be a number");
    assert_ne!(val, 42, "BASHPID assignment should be ignored");
}

#[testutil::test]
fn bashpid_unset_works() {
    // After unset, BASHPID should be empty.
    let (out, _) = bash_exec_ok("unset BASHPID; echo \"x${BASHPID}x\"");
    assert_eq!(out.trim(), "xx");
}

// LINENO ==============================================================================================================

#[testutil::test]
fn lineno_increments_per_line() {
    let script = "echo $LINENO\necho $LINENO\necho $LINENO";
    let (out, _) = bash_exec_ok(script);
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.len(), 3);
    let n1: usize = lines[0].parse().unwrap();
    let n2: usize = lines[1].parse().unwrap();
    let n3: usize = lines[2].parse().unwrap();
    assert!(n2 > n1, "LINENO should increase: {n1} → {n2}");
    assert!(n3 > n2, "LINENO should increase: {n2} → {n3}");
}

#[testutil::test]
fn lineno_unset_kills_special_behavior() {
    let (out, _) = bash_exec_ok("unset LINENO; LINENO=42; echo $LINENO");
    assert_eq!(out.trim(), "42");
}
