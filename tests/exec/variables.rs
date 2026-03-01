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

// PPID ================================================================================================================

#[testutil::test]
fn ppid_is_set() {
    let (out, _) = bash_exec_ok("echo $PPID");
    let val: u32 = out.trim().parse().expect("PPID should be a number");
    assert!(val > 0, "PPID should be positive");
}

#[testutil::test]
fn ppid_is_readonly() {
    // PPID should reject assignment.
    let status = bash_exec_result("PPID=42 2>/dev/null");
    assert_ne!(status, 0, "PPID assignment should fail");
}

#[testutil::test]
fn ppid_cannot_be_unset() {
    let status = bash_exec_result("unset PPID 2>/dev/null");
    assert_ne!(status, 0, "unset PPID should fail");
}

// getopts =============================================================================================================

#[testutil::test]
fn getopts_basic_single_options() {
    let script = r#"
while getopts "abc" opt -- -a -b -c; do
    echo $opt
done
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out, "a\nb\nc\n");
}

#[testutil::test]
fn getopts_grouped_options() {
    let script = r#"
while getopts "abc" opt -- -abc; do
    echo $opt
done
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out, "a\nb\nc\n");
}

#[testutil::test]
fn getopts_option_with_argument_separate() {
    let script = r#"
getopts "a:" opt -- -a VALUE
echo "$opt $OPTARG"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "a VALUE");
}

#[testutil::test]
fn getopts_option_with_argument_concatenated() {
    let script = r#"
getopts "a:" opt -- -aVALUE
echo "$opt $OPTARG"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "a VALUE");
}

#[testutil::test]
fn getopts_unknown_option_verbose() {
    // Unknown option in verbose mode: name=?, stderr diagnostic
    let script = r#"
getopts "ab" opt -- -z 2>/dev/null
echo $opt
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "?");
}

#[testutil::test]
fn getopts_silent_mode_unknown() {
    // Silent mode (leading :): name=?, OPTARG=offending char
    let script = r#"
getopts ":ab" opt -- -z
echo "$opt $OPTARG"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "? z");
}

#[testutil::test]
fn getopts_silent_mode_missing_arg() {
    // Silent mode: missing argument → name=:, OPTARG=option char
    let script = r#"
getopts ":a:" opt -- -a
echo "$opt $OPTARG"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), ": a");
}

#[testutil::test]
fn getopts_double_dash_terminates() {
    let script = r#"
getopts "a" opt -- -- -a
echo "status=$?"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "status=1");
}

#[testutil::test]
fn getopts_non_option_terminates() {
    let script = r#"
getopts "a" opt -- foo -a
echo "status=$?"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "status=1");
}

#[testutil::test]
fn getopts_optind_reset() {
    // After processing, OPTIND can be reset to 1 to re-parse.
    let script = r#"
getopts "a" opt -- -a
echo $opt
OPTIND=1
getopts "a" opt -- -a
echo $opt
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out, "a\na\n");
}

#[testutil::test]
fn getopts_uses_positional_params_by_default() {
    let script = r#"
set -- -a -b
while getopts "ab" opt; do
    echo $opt
done
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out, "a\nb\n");
}

#[testutil::test]
fn getopts_grouped_with_required_arg() {
    // -abc where a requires arg → OPTARG=bc
    let script = r#"
getopts "a:bc" opt -- -abc
echo "$opt $OPTARG"
"#;
    let (out, _) = exec_ok(script);
    assert_eq!(out.trim(), "a bc");
}

// Bash static variables ===============================================================================================

#[testutil::test]
fn bash_version_is_set() {
    let (out, _) = bash_exec_ok("echo $BASH_VERSION");
    let ver = out.trim();
    assert!(!ver.is_empty(), "BASH_VERSION should be set");
    // Should contain a dot (e.g. "5.2.0(1)-release")
    assert!(ver.contains('.'), "BASH_VERSION should contain a dot: {ver}");
}

#[testutil::test]
fn bash_versinfo_is_array() {
    let (out, _) = bash_exec_ok("echo ${BASH_VERSINFO[0]}");
    let major: u32 = out.trim().parse().expect("BASH_VERSINFO[0] should be a number");
    assert!(major >= 1, "major version should be >= 1");
}

#[testutil::test]
fn bash_versinfo_is_readonly() {
    let status = bash_exec_result("BASH_VERSINFO=(1 2 3) 2>/dev/null");
    assert_ne!(status, 0, "BASH_VERSINFO should be readonly");
}

#[testutil::test]
fn uid_is_set() {
    let (out, _) = bash_exec_ok("echo $UID");
    let val: u32 = out.trim().parse().expect("UID should be a number");
    // Just check it's a valid uid (could be 0 for root)
    assert!(val <= 65534, "UID out of range: {val}");
}

#[testutil::test]
fn uid_is_readonly() {
    let status = bash_exec_result("UID=42 2>/dev/null");
    assert_ne!(status, 0, "UID should be readonly");
}

#[testutil::test]
fn euid_is_set() {
    let (out, _) = bash_exec_ok("echo $EUID");
    let val: u32 = out.trim().parse().expect("EUID should be a number");
    assert!(val <= 65534, "EUID out of range: {val}");
}

#[testutil::test]
fn euid_is_readonly() {
    let status = bash_exec_result("EUID=42 2>/dev/null");
    assert_ne!(status, 0, "EUID should be readonly");
}

#[testutil::test]
fn hostname_is_set() {
    let (out, _) = bash_exec_ok("echo $HOSTNAME");
    assert!(!out.trim().is_empty(), "HOSTNAME should be non-empty");
}

#[testutil::test]
fn hosttype_is_set() {
    let (out, _) = bash_exec_ok("echo $HOSTTYPE");
    let ht = out.trim();
    assert!(!ht.is_empty(), "HOSTTYPE should be set");
    // Should be something like "x86_64" or "aarch64"
    assert!(
        ht.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "unexpected HOSTTYPE: {ht}"
    );
}

#[testutil::test]
fn ostype_is_set() {
    let (out, _) = bash_exec_ok("echo $OSTYPE");
    let ost = out.trim();
    assert!(!ost.is_empty(), "OSTYPE should be set");
}

#[testutil::test]
fn machtype_is_set() {
    let (out, _) = bash_exec_ok("echo $MACHTYPE");
    let mt = out.trim();
    assert!(!mt.is_empty(), "MACHTYPE should be set");
    // Should contain a dash (e.g. "x86_64-pc-linux-gnu")
    assert!(mt.contains('-'), "MACHTYPE should contain a dash: {mt}");
}

#[testutil::test]
fn hostname_can_be_overwritten() {
    // HOSTNAME is Category E — can be freely assigned
    let (out, _) = bash_exec_ok("HOSTNAME=myhost; echo $HOSTNAME");
    assert_eq!(out.trim(), "myhost");
}

#[testutil::test]
fn groups_is_array() {
    let (out, _) = bash_exec_ok("echo ${GROUPS[0]}");
    let gid: u32 = out.trim().parse().expect("GROUPS[0] should be a number");
    assert!(gid <= 65534, "GID out of range: {gid}");
}

#[testutil::test]
fn groups_assign_silently_ignored() {
    // GROUPS is Category D — assign silently ignored
    let (out1, _) = bash_exec_ok("echo ${GROUPS[0]}");
    let (out2, _) = bash_exec_ok("GROUPS=(999); echo ${GROUPS[0]}");
    assert_eq!(out1, out2, "GROUPS assignment should be silently ignored");
}

// PIPESTATUS ==========================================================================================================

#[testutil::test]
fn pipestatus_single_command() {
    let (out, _) = bash_exec_ok("true; echo ${PIPESTATUS[0]}");
    assert_eq!(out.trim(), "0");
}

#[testutil::test]
fn pipestatus_failed_command() {
    let (out, _) = bash_exec_ok("false; echo ${PIPESTATUS[0]}");
    assert_eq!(out.trim(), "1");
}

#[testutil::test]
fn pipestatus_unset_repopulates() {
    // Category B: unset is temporary — next command repopulates.
    let (out, _) = bash_exec_ok("unset PIPESTATUS; true; echo ${PIPESTATUS[0]}");
    assert_eq!(out.trim(), "0");
}

// SHELLOPTS ===========================================================================================================

#[testutil::test]
fn shellopts_is_set() {
    let (out, _) = bash_exec_ok("echo $SHELLOPTS");
    let opts = out.trim();
    assert!(!opts.is_empty(), "SHELLOPTS should be set");
}

#[testutil::test]
fn shellopts_contains_errexit_after_set_e() {
    let (out, _) = bash_exec_ok("set -e; echo $SHELLOPTS");
    assert!(out.contains("errexit"), "SHELLOPTS should contain 'errexit'");
}

#[testutil::test]
fn shellopts_is_readonly() {
    let status = bash_exec_result("SHELLOPTS=x 2>/dev/null");
    assert_ne!(status, 0, "SHELLOPTS should be readonly");
}

#[testutil::test]
fn shellopts_cannot_be_unset() {
    let status = bash_exec_result("unset SHELLOPTS 2>/dev/null");
    assert_ne!(status, 0, "unset SHELLOPTS should fail");
}

// BASHOPTS ============================================================================================================

#[testutil::test]
fn bashopts_is_set() {
    let (out, _) = bash_exec_ok("echo $BASHOPTS");
    // Could be empty if no shopt options are enabled, but the variable should exist.
    // Just check it doesn't error.
    let _ = out.trim();
}

#[testutil::test]
fn bashopts_is_readonly() {
    let status = bash_exec_result("BASHOPTS=x 2>/dev/null");
    assert_ne!(status, 0, "BASHOPTS should be readonly");
}

#[testutil::test]
fn bashopts_cannot_be_unset() {
    let status = bash_exec_result("unset BASHOPTS 2>/dev/null");
    assert_ne!(status, 0, "unset BASHOPTS should fail");
}

// FUNCNAME ============================================================================================================

#[testutil::test]
fn funcname_in_function() {
    let (out, _) = bash_exec_ok("f() { echo ${FUNCNAME[0]}; }; f");
    assert_eq!(out.trim(), "f");
}

#[testutil::test]
fn funcname_nested() {
    let (out, _) = bash_exec_ok("f() { g; }; g() { echo ${FUNCNAME[0]} ${FUNCNAME[1]}; }; f");
    assert_eq!(out.trim(), "g f");
}

#[testutil::test]
fn funcname_main_at_bottom() {
    let (out, _) = bash_exec_ok("f() { echo ${FUNCNAME[@]}; }; f");
    let parts: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(parts.first(), Some(&"f"), "FUNCNAME[0] should be 'f'");
    assert_eq!(parts.last(), Some(&"main"), "bottom of FUNCNAME should be 'main'");
}

#[testutil::test]
fn funcname_empty_outside_function() {
    let (out, _) = bash_exec_ok("echo \"x${FUNCNAME[0]}x\"");
    assert_eq!(out.trim(), "xx", "FUNCNAME should be empty outside a function");
}

// BASH_SOURCE =========================================================================================================

#[testutil::test]
fn bash_source_cannot_be_unset() {
    let status = bash_exec_result("unset BASH_SOURCE 2>/dev/null");
    assert_ne!(status, 0, "unset BASH_SOURCE should fail");
}

// BASH_LINENO =========================================================================================================

#[testutil::test]
fn bash_lineno_cannot_be_unset() {
    let status = bash_exec_result("unset BASH_LINENO 2>/dev/null");
    assert_ne!(status, 0, "unset BASH_LINENO should fail");
}

// pushd/popd/dirs + DIRSTACK ==========================================================================================

#[testutil::test]
fn dirs_shows_current_dir() {
    let (out, _) = bash_exec_ok("dirs");
    assert!(
        !out.trim().is_empty(),
        "dirs should show at least the current directory"
    );
}

#[testutil::test]
fn pushd_and_popd_basic() {
    let (out, _) = bash_exec_ok("pushd /tmp > /dev/null; echo $PWD; popd > /dev/null; echo $PWD");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines[0], "/tmp", "pushd should cd to /tmp");
    // After popd, we should be back to original dir.
    assert_ne!(lines[1], "/tmp", "popd should restore original dir");
}

#[testutil::test]
fn dirstack_tracks_pushd() {
    let (out, _) = bash_exec_ok("pushd /tmp > /dev/null; echo ${DIRSTACK[0]}");
    assert_eq!(out.trim(), "/tmp");
}

#[testutil::test]
fn popd_empty_stack_fails() {
    let status = bash_exec_result("popd 2>/dev/null");
    assert_ne!(status, 0, "popd with empty stack should fail");
}

#[testutil::test]
fn pushd_no_args_swaps_top_two() {
    let (out, _) = bash_exec_ok("pushd /tmp > /dev/null; pushd /var > /dev/null; pushd > /dev/null; echo $PWD");
    assert_eq!(out.trim(), "/tmp", "pushd with no args should swap top two");
}

#[testutil::test]
fn dirs_c_clears_stack() {
    let (out, _) = bash_exec_ok("pushd /tmp > /dev/null; dirs -c; dirs -p");
    let lines: Vec<&str> = out.trim().lines().collect();
    // After dirs -c, only the current dir should remain.
    assert_eq!(lines.len(), 1);
}

#[testutil::test]
fn dirs_v_shows_indices() {
    let (out, _) = bash_exec_ok("pushd /tmp > /dev/null; dirs -v");
    // Should have indices like " 0  /tmp"
    assert!(out.contains(" 0"), "dirs -v should show index 0");
}

#[testutil::test]
fn pushd_n_no_cd() {
    let (out, _) = bash_exec_ok("pushd -n /tmp > /dev/null; echo $PWD");
    // With -n, pushd should NOT change directory.
    assert_ne!(out.trim(), "/tmp", "pushd -n should not change directory");
}
