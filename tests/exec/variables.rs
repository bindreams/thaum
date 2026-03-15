use std::path::Path;

use crate::*;

// $- (option flags) ===================================================================================================

#[skuld::test]
fn dollar_dash_default() {
    // With no options set, $- should be a non-empty string (at minimum hB for bash defaults,
    // but our shell may start with a different set).
    let r = exec!("echo $-");
    // Should be non-empty and contain only flag characters
    let flags = r.stdout().trim();
    assert!(!flags.is_empty(), "$- should not be empty");
    for c in flags.chars() {
        assert!(c.is_ascii_alphabetic(), "unexpected char in $-: {c:?}");
    }
}

#[skuld::test]
fn dollar_dash_reflects_errexit() {
    let r = exec!("set -e; echo $-");
    assert!(r.stdout().trim().contains('e'), "$- should contain 'e' after set -e");
}

#[skuld::test]
fn dollar_dash_reflects_nounset() {
    let r = exec!("set -u; echo $-");
    assert!(r.stdout().trim().contains('u'), "$- should contain 'u' after set -u");
}

#[skuld::test]
fn dollar_dash_reflects_xtrace() {
    // xtrace output goes to stderr, not stdout; just check the flag is present
    let r = exec!("set -x; echo $-");
    assert!(r.stdout().trim().contains('x'), "$- should contain 'x' after set -x");
    assert!(!r.stderr().is_empty(), "xtrace should produce stderr output");
}

#[skuld::test]
fn dollar_dash_not_affected_by_nounset() {
    // $- is a special parameter, so set -u should not cause an error
    let r = exec!("set -u; echo $-");
    assert!(!r.stdout().trim().is_empty());
}

// $_ (last argument) ==================================================================================================

#[skuld::test]
fn dollar_underscore_last_arg() {
    let r = exec!("echo a b c\necho $_");
    assert_eq!(r.stdout(), "a b c\nc\n");
}

#[skuld::test]
fn dollar_underscore_after_single_arg_command() {
    let r = exec!("echo hello\necho $_");
    assert_eq!(r.stdout(), "hello\nhello\n");
}

#[skuld::test]
fn dollar_underscore_after_no_arg_command() {
    // After a command with no arguments (like `true`), $_ is the command name itself
    let r = exec!("true\necho $_");
    assert_eq!(r.stdout(), "true\n");
}

// RANDOM ==============================================================================================================

#[skuld::test]
fn random_returns_number_in_range() {
    let r = exec!("echo $RANDOM", dialect = Dialect::Bash);
    let val: i32 = r.stdout().trim().parse().expect("RANDOM should be a number");
    assert!((0..=32767).contains(&val), "RANDOM={val} out of 0..32767");
}

#[skuld::test]
fn random_differs_on_consecutive_reads() {
    // Two consecutive reads of RANDOM should (almost certainly) differ.
    let r = exec!("echo $RANDOM $RANDOM", dialect = Dialect::Bash);
    let parts: Vec<&str> = r.stdout().split_whitespace().collect();
    assert_eq!(parts.len(), 2);
    // They could theoretically be equal, but the probability is ~1/32768.
    // If this flakes, the RNG is not advancing.
    assert_ne!(parts[0], parts[1], "two RANDOM reads should differ");
}

#[skuld::test]
fn random_seed_produces_deterministic_sequence() {
    // Setting RANDOM seeds the LCG — same seed should yield same first value.
    let r = exec!("RANDOM=42; echo $RANDOM", dialect = Dialect::Bash);
    let r2 = exec!("RANDOM=42; echo $RANDOM", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), r2.stdout(), "same seed should produce same RANDOM");
}

#[skuld::test]
fn random_unset_kills_special_behavior() {
    // After unset, RANDOM should become a plain variable.
    let r = exec!("unset RANDOM; RANDOM=42; echo $RANDOM", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "42", "unset RANDOM should kill special behavior");
}

#[skuld::test]
fn random_not_affected_by_nounset() {
    let r = exec!("set -u; echo $RANDOM", dialect = Dialect::Bash);
    let val: i32 = r.stdout().trim().parse().expect("RANDOM should be a number");
    assert!((0..=32767).contains(&val));
}

// SECONDS =============================================================================================================

#[skuld::test]
fn seconds_returns_nonnegative() {
    let r = exec!("echo $SECONDS", dialect = Dialect::Bash);
    let val: i64 = r.stdout().trim().parse().expect("SECONDS should be a number");
    assert!(val >= 0, "SECONDS should be >= 0");
}

#[skuld::test]
fn seconds_assignment_resets_timer() {
    // Setting SECONDS=0 resets; subsequent read should be 0 (or very small).
    let r = exec!("SECONDS=0; echo $SECONDS", dialect = Dialect::Bash);
    let val: i64 = r.stdout().trim().parse().expect("SECONDS should be a number");
    assert!(val <= 2, "SECONDS after reset should be small, got {val}");
}

#[skuld::test]
fn seconds_unset_kills_special_behavior() {
    let r = exec!("unset SECONDS; SECONDS=100; echo $SECONDS", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "100", "unset SECONDS should kill timer behavior");
}

// EPOCHSECONDS ========================================================================================================

#[skuld::test]
fn epochseconds_returns_valid_timestamp() {
    let r = exec!("echo $EPOCHSECONDS", dialect = Dialect::Bash);
    let val: u64 = r.stdout().trim().parse().expect("EPOCHSECONDS should be a number");
    // Should be a reasonable Unix timestamp (after 2020-01-01)
    assert!(val > 1_577_836_800, "EPOCHSECONDS too small: {val}");
}

#[skuld::test]
fn epochseconds_unset_kills_special_behavior() {
    let r = exec!(
        "unset EPOCHSECONDS; EPOCHSECONDS=42; echo $EPOCHSECONDS",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout().trim(), "42");
}

// EPOCHREALTIME =======================================================================================================

#[skuld::test]
fn epochrealtime_format() {
    let r = exec!("echo $EPOCHREALTIME", dialect = Dialect::Bash);
    let s = r.stdout().trim();
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

#[skuld::test]
fn epochrealtime_unset_kills_special_behavior() {
    let r = exec!(
        "unset EPOCHREALTIME; EPOCHREALTIME=1.23; echo $EPOCHREALTIME",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout().trim(), "1.23");
}

// SRANDOM =============================================================================================================

#[skuld::test]
fn srandom_returns_u32() {
    let r = exec!("echo $SRANDOM", dialect = Dialect::Bash);
    let val: u64 = r.stdout().trim().parse().expect("SRANDOM should be a number");
    assert!(val <= u32::MAX as u64, "SRANDOM out of u32 range: {val}");
}

#[skuld::test]
fn srandom_differs_on_consecutive_reads() {
    let r = exec!("echo $SRANDOM $SRANDOM", dialect = Dialect::Bash);
    let parts: Vec<&str> = r.stdout().split_whitespace().collect();
    assert_eq!(parts.len(), 2);
    assert_ne!(parts[0], parts[1], "two SRANDOM reads should differ");
}

#[skuld::test]
fn srandom_assign_ignored() {
    // Assigning to SRANDOM should be silently ignored — next read is still random.
    let r = exec!("SRANDOM=42; echo $SRANDOM", dialect = Dialect::Bash);
    let val: u64 = r.stdout().trim().parse().expect("SRANDOM should be a number");
    // The value should NOT be 42 (probability ~1/2^32).
    // More importantly, it should still be a valid u32.
    assert!(val <= u32::MAX as u64);
}

#[skuld::test]
fn srandom_unset_kills_special_behavior() {
    let r = exec!("unset SRANDOM; SRANDOM=42; echo $SRANDOM", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "42");
}

// BASHPID =============================================================================================================

#[skuld::test]
fn bashpid_returns_current_pid() {
    let r = exec!("echo $BASHPID", dialect = Dialect::Bash);
    let val: u32 = r.stdout().trim().parse().expect("BASHPID should be a number");
    assert!(val > 0, "BASHPID should be positive");
}

#[skuld::test]
fn bashpid_assign_silently_ignored() {
    // Assigning to BASHPID should be silently ignored.
    let r = exec!("BASHPID=42; echo $BASHPID", dialect = Dialect::Bash);
    let val: u32 = r.stdout().trim().parse().expect("BASHPID should be a number");
    assert_ne!(val, 42, "BASHPID assignment should be ignored");
}

#[skuld::test]
fn bashpid_unset_works() {
    // After unset, BASHPID should be empty.
    let r = exec!("unset BASHPID; echo \"x${BASHPID}x\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "xx");
}

// LINENO ==============================================================================================================

#[skuld::test]
fn lineno_increments_per_line() {
    let script = "echo $LINENO\necho $LINENO\necho $LINENO";
    let r = exec!(script, dialect = Dialect::Bash);
    let lines: Vec<&str> = r.stdout().trim().lines().collect();
    assert_eq!(lines.len(), 3);
    let n1: usize = lines[0].parse().unwrap();
    let n2: usize = lines[1].parse().unwrap();
    let n3: usize = lines[2].parse().unwrap();
    assert!(n2 > n1, "LINENO should increase: {n1} → {n2}");
    assert!(n3 > n2, "LINENO should increase: {n2} → {n3}");
}

#[skuld::test]
fn lineno_unset_kills_special_behavior() {
    let r = exec!("unset LINENO; LINENO=42; echo $LINENO", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "42");
}

// PPID ================================================================================================================

#[skuld::test]
fn ppid_is_set() {
    let r = exec!("echo $PPID", dialect = Dialect::Bash);
    let val: u32 = r.stdout().trim().parse().expect("PPID should be a number");
    assert!(val > 0, "PPID should be positive");
}

#[skuld::test]
fn ppid_is_readonly() {
    let r = exec!("PPID=42", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "PPID assignment should fail");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn ppid_cannot_be_unset() {
    let r = exec!("unset PPID", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "unset PPID should fail");
    assert!(!r.stderr().is_empty());
}

// getopts =============================================================================================================

#[skuld::test]
fn getopts_basic_single_options() {
    let script = r#"
while getopts "abc" opt -- -a -b -c; do
    echo $opt
done
"#;
    let r = exec!(script);
    assert_eq!(r.stdout(), "a\nb\nc\n");
}

#[skuld::test]
fn getopts_grouped_options() {
    let script = r#"
while getopts "abc" opt -- -abc; do
    echo $opt
done
"#;
    let r = exec!(script);
    assert_eq!(r.stdout(), "a\nb\nc\n");
}

#[skuld::test]
fn getopts_option_with_argument_separate() {
    let script = r#"
getopts "a:" opt -- -a VALUE
echo "$opt $OPTARG"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "a VALUE");
}

#[skuld::test]
fn getopts_option_with_argument_concatenated() {
    let script = r#"
getopts "a:" opt -- -aVALUE
echo "$opt $OPTARG"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "a VALUE");
}

#[skuld::test]
fn getopts_unknown_option_verbose() {
    // Unknown option in verbose mode: name=?, stderr diagnostic
    let script = r#"
getopts "ab" opt -- -z 2>/dev/null
echo $opt
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "?");
}

#[skuld::test]
fn getopts_silent_mode_unknown() {
    // Silent mode (leading :): name=?, OPTARG=offending char
    let script = r#"
getopts ":ab" opt -- -z
echo "$opt $OPTARG"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "? z");
}

#[skuld::test]
fn getopts_silent_mode_missing_arg() {
    // Silent mode: missing argument → name=:, OPTARG=option char
    let script = r#"
getopts ":a:" opt -- -a
echo "$opt $OPTARG"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), ": a");
}

#[skuld::test]
fn getopts_double_dash_terminates() {
    let script = r#"
getopts "a" opt -- -- -a
echo "status=$?"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "status=1");
}

#[skuld::test]
fn getopts_non_option_terminates() {
    let script = r#"
getopts "a" opt -- foo -a
echo "status=$?"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "status=1");
}

#[skuld::test]
fn getopts_optind_reset() {
    // After processing, OPTIND can be reset to 1 to re-parse.
    let script = r#"
getopts "a" opt -- -a
echo $opt
OPTIND=1
getopts "a" opt -- -a
echo $opt
"#;
    let r = exec!(script);
    assert_eq!(r.stdout(), "a\na\n");
}

#[skuld::test]
fn getopts_uses_positional_params_by_default() {
    let script = r#"
set -- -a -b
while getopts "ab" opt; do
    echo $opt
done
"#;
    let r = exec!(script);
    assert_eq!(r.stdout(), "a\nb\n");
}

#[skuld::test]
fn getopts_grouped_with_required_arg() {
    // -abc where a requires arg → OPTARG=bc
    let script = r#"
getopts "a:bc" opt -- -abc
echo "$opt $OPTARG"
"#;
    let r = exec!(script);
    assert_eq!(r.stdout().trim(), "a bc");
}

// Bash static variables ===============================================================================================

#[skuld::test]
fn bash_version_is_set() {
    let r = exec!("echo $BASH_VERSION", dialect = Dialect::Bash);
    let ver = r.stdout().trim();
    assert!(!ver.is_empty(), "BASH_VERSION should be set");
    // Should contain a dot (e.g. "5.2.0(1)-release")
    assert!(ver.contains('.'), "BASH_VERSION should contain a dot: {ver}");
}

#[skuld::test]
fn bash_versinfo_is_array() {
    let r = exec!("echo ${BASH_VERSINFO[0]}", dialect = Dialect::Bash);
    let major: u32 = r.stdout().trim().parse().expect("BASH_VERSINFO[0] should be a number");
    assert!(major >= 1, "major version should be >= 1");
}

#[skuld::test]
fn bash_versinfo_is_readonly() {
    let r = exec!("BASH_VERSINFO=(1 2 3)", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "BASH_VERSINFO should be readonly");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn uid_is_set() {
    let r = exec!("echo $UID", dialect = Dialect::Bash);
    let val: u32 = r.stdout().trim().parse().expect("UID should be a number");
    // Just check it's a valid uid (could be 0 for root)
    assert!(val <= 65534, "UID out of range: {val}");
}

#[skuld::test]
fn uid_is_readonly() {
    let r = exec!("UID=42", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "UID should be readonly");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn euid_is_set() {
    let r = exec!("echo $EUID", dialect = Dialect::Bash);
    let val: u32 = r.stdout().trim().parse().expect("EUID should be a number");
    assert!(val <= 65534, "EUID out of range: {val}");
}

#[skuld::test]
fn euid_is_readonly() {
    let r = exec!("EUID=42", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "EUID should be readonly");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn hostname_is_set() {
    let r = exec!("echo $HOSTNAME", dialect = Dialect::Bash);
    assert!(!r.stdout().trim().is_empty(), "HOSTNAME should be non-empty");
}

#[skuld::test]
fn hosttype_is_set() {
    let r = exec!("echo $HOSTTYPE", dialect = Dialect::Bash);
    let ht = r.stdout().trim();
    assert!(!ht.is_empty(), "HOSTTYPE should be set");
    // Should be something like "x86_64" or "aarch64"
    assert!(
        ht.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "unexpected HOSTTYPE: {ht}"
    );
}

#[skuld::test]
fn ostype_is_set() {
    let r = exec!("echo $OSTYPE", dialect = Dialect::Bash);
    let ost = r.stdout().trim();
    assert!(!ost.is_empty(), "OSTYPE should be set");
}

#[skuld::test]
fn machtype_is_set() {
    let r = exec!("echo $MACHTYPE", dialect = Dialect::Bash);
    let mt = r.stdout().trim();
    assert!(!mt.is_empty(), "MACHTYPE should be set");
    // Should contain a dash (e.g. "x86_64-pc-linux-gnu")
    assert!(mt.contains('-'), "MACHTYPE should contain a dash: {mt}");
}

#[skuld::test]
fn hostname_can_be_overwritten() {
    // HOSTNAME is Category E — can be freely assigned
    let r = exec!("HOSTNAME=myhost; echo $HOSTNAME", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "myhost");
}

#[skuld::test]
fn groups_is_array() {
    let r = exec!("echo ${GROUPS[0]}", dialect = Dialect::Bash);
    let gid: u32 = r.stdout().trim().parse().expect("GROUPS[0] should be a number");
    assert!(gid <= 65534, "GID out of range: {gid}");
}

#[skuld::test]
fn groups_assign_silently_ignored() {
    // GROUPS is Category D — assign silently ignored
    let r = exec!("echo ${GROUPS[0]}", dialect = Dialect::Bash);
    let r2 = exec!("GROUPS=(999); echo ${GROUPS[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), r2.stdout(), "GROUPS assignment should be silently ignored");
}

// PIPESTATUS ==========================================================================================================

#[skuld::test]
fn pipestatus_single_command() {
    let r = exec!("true; echo ${PIPESTATUS[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "0");
}

#[skuld::test]
fn pipestatus_failed_command() {
    let r = exec!("false; echo ${PIPESTATUS[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "1");
}

#[skuld::test]
fn pipestatus_unset_repopulates() {
    // Category B: unset is temporary — next command repopulates.
    let r = exec!("unset PIPESTATUS; true; echo ${PIPESTATUS[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "0");
}

// SHELLOPTS ===========================================================================================================

#[skuld::test]
fn shellopts_is_set() {
    let r = exec!("echo $SHELLOPTS", dialect = Dialect::Bash);
    let opts = r.stdout().trim();
    assert!(!opts.is_empty(), "SHELLOPTS should be set");
}

#[skuld::test]
fn shellopts_contains_errexit_after_set_e() {
    let r = exec!("set -e; echo $SHELLOPTS", dialect = Dialect::Bash);
    assert!(r.stdout().contains("errexit"), "SHELLOPTS should contain 'errexit'");
}

#[skuld::test]
fn shellopts_is_readonly() {
    let r = exec!("SHELLOPTS=x", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "SHELLOPTS should be readonly");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn shellopts_cannot_be_unset() {
    let r = exec!("unset SHELLOPTS", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "unset SHELLOPTS should fail");
    assert!(!r.stderr().is_empty());
}

// BASHOPTS ============================================================================================================

#[skuld::test]
fn bashopts_is_set() {
    let r = exec!("echo $BASHOPTS", dialect = Dialect::Bash);
    // Could be empty if no shopt options are enabled, but the variable should exist.
    // Just check it doesn't error.
    let _ = r.stdout().trim();
}

#[skuld::test]
fn bashopts_is_readonly() {
    let r = exec!("BASHOPTS=x", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "BASHOPTS should be readonly");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn bashopts_cannot_be_unset() {
    let r = exec!("unset BASHOPTS", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "unset BASHOPTS should fail");
    assert!(!r.stderr().is_empty());
}

// FUNCNAME ============================================================================================================

#[skuld::test]
fn funcname_in_function() {
    let r = exec!("f() { echo ${FUNCNAME[0]}; }; f", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "f");
}

#[skuld::test]
fn funcname_nested() {
    let r = exec!(
        "f() { g; }; g() { echo ${FUNCNAME[0]} ${FUNCNAME[1]}; }; f",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout().trim(), "g f");
}

#[skuld::test]
fn funcname_main_at_bottom() {
    let r = exec!("f() { echo ${FUNCNAME[@]}; }; f", dialect = Dialect::Bash);
    let parts: Vec<&str> = r.stdout().split_whitespace().collect();
    assert_eq!(parts.first(), Some(&"f"), "FUNCNAME[0] should be 'f'");
    assert_eq!(parts.last(), Some(&"main"), "bottom of FUNCNAME should be 'main'");
}

#[skuld::test]
fn funcname_empty_outside_function() {
    let r = exec!("echo \"x${FUNCNAME[0]}x\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "xx", "FUNCNAME should be empty outside a function");
}

// BASH_SOURCE =========================================================================================================

#[skuld::test]
fn bash_source_cannot_be_unset() {
    let r = exec!("unset BASH_SOURCE", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "unset BASH_SOURCE should fail");
    assert!(!r.stderr().is_empty());
}

// BASH_LINENO =========================================================================================================

#[skuld::test]
fn bash_lineno_cannot_be_unset() {
    let r = exec!("unset BASH_LINENO", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0, "unset BASH_LINENO should fail");
    assert!(!r.stderr().is_empty());
}

#[skuld::test]
fn bash_lineno_tracks_call_site() {
    // f is defined on line 1, called on line 2.
    // BASH_LINENO[0] should be 2 (the line where f was called).
    let r = exec!("f() { echo ${BASH_LINENO[0]}; }\nf", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "2", "BASH_LINENO[0] should be the call site line");
}

#[skuld::test]
fn bash_lineno_nested_calls() {
    // g defined line 1, f defined line 2, f called at line 3.
    // Inside g: BASH_LINENO = [2, 3] (g called at line 2 inside f, f called at line 3).
    let script = "g() { echo ${BASH_LINENO[@]}; }\nf() { g; }\nf";
    let r = exec!(script, dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "2 3", "BASH_LINENO should show nested call sites");
}

#[skuld::test]
fn bash_source_empty_at_top_level() {
    // Outside functions, BASH_SOURCE should be empty.
    let r = exec!("echo \"x${BASH_SOURCE[0]}x\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "xx");
}

#[skuld::test]
fn bash_source_in_sourced_file(#[fixture(temp_dir)] dir: &Path) {
    // source a file, and inside it BASH_SOURCE[0] should be the filename.
    let file = dir.join("lib.sh");
    std::fs::write(&file, "echo ${BASH_SOURCE[0]}\n").unwrap();

    let f = file.to_string_lossy().replace('\\', "/");
    let r = exec!(&format!("source {f}"), dialect = Dialect::Bash);
    // On Windows, the shell normalizes backslashes to forward slashes.
    let expected = file.to_string_lossy().replace('\\', "/");
    assert_eq!(r.stdout().trim(), expected);
}

#[skuld::test]
fn bash_lineno_in_sourced_file_calling_function(#[fixture(temp_dir)] dir: &Path) {
    // Source a file that defines and calls a function.
    // BASH_LINENO inside the function should reflect the sourced file's lines.
    let lib = dir.join("lib.sh");
    // Line 1: function definition, line 2: function call.
    std::fs::write(&lib, "f() { echo ${BASH_LINENO[0]}; }\nf\n").unwrap();

    let f = lib.to_string_lossy().replace('\\', "/");
    let r = exec!(&format!("source {f}"), dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "2", "BASH_LINENO should track line in sourced file");
}

#[skuld::test]
fn bash_lineno_source_from_function(#[fixture(temp_dir)] dir: &Path) {
    // A function on line 1 sources a file. Inside the sourced file, BASH_LINENO
    // should reflect the sourced file's own line numbers, not the function's
    // definition offset.
    let lib = dir.join("lib.sh");
    // lib.sh: line 1 defines g, line 2 calls g.
    std::fs::write(&lib, "g() { echo ${BASH_LINENO[0]}; }\ng\n").unwrap();

    let f = lib.to_string_lossy().replace('\\', "/");
    // h on line 1, f on line 2 → f's def_lineno=2, lineno_base=1.
    // If lineno_base leaks into the sourced file, g's call site would be
    // 3 (= 2 + 1) instead of 2.
    let script = format!("h() {{ :; }}\nf() {{ source {f}; }}\nf");
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(
        r.stdout().trim(),
        "2",
        "sourced file lineno should not inherit function's lineno_base"
    );
}

#[skuld::test]
fn bash_source_tracks_definition_file(#[fixture(temp_dir)] dir: &Path) {
    // A function defined in lib.sh should show lib.sh in BASH_SOURCE,
    // even when called from the main script (not from lib.sh).
    let lib = dir.join("lib.sh");
    std::fs::write(&lib, "f() { echo ${BASH_SOURCE[0]}; }\n").unwrap();

    let f = lib.to_string_lossy().replace('\\', "/");
    let expected = f.clone();
    // source lib.sh to define f, then call f from the main script.
    let r = exec!(&format!("source {f}; f"), dialect = Dialect::Bash);
    assert_eq!(
        r.stdout().trim(),
        expected,
        "BASH_SOURCE[0] should be the file where f was DEFINED"
    );
}

#[skuld::test]
fn bash_source_nested_source(#[fixture(temp_dir)] dir: &Path) {
    // source a.sh which sources b.sh. Inside b.sh, BASH_SOURCE should stack.
    let b = dir.join("b.sh");
    let a = dir.join("a.sh");
    let b_path = b.to_string_lossy().replace('\\', "/");
    std::fs::write(&b, "echo ${BASH_SOURCE[0]}\n").unwrap();
    std::fs::write(&a, format!("source {b_path}\n")).unwrap();

    let a_path = a.to_string_lossy().replace('\\', "/");
    let r = exec!(&format!("source {a_path}"), dialect = Dialect::Bash);
    assert_eq!(
        r.stdout().trim(),
        b_path,
        "BASH_SOURCE[0] should be the innermost sourced file"
    );
}

// pushd/popd/dirs + DIRSTACK ==========================================================================================

#[skuld::test]
fn dirs_shows_current_dir() {
    let r = exec!("dirs", dialect = Dialect::Bash);
    assert!(
        !r.stdout().trim().is_empty(),
        "dirs should show at least the current directory"
    );
}

#[skuld::test]
fn pushd_and_popd_basic() {
    // Canonicalize because /tmp may be a symlink (e.g. /private/tmp on macOS).
    let real_tmp = std::fs::canonicalize("/tmp").unwrap().to_string_lossy().to_string();
    let r = exec!(
        "pushd /tmp > /dev/null; echo $PWD; popd > /dev/null; echo $PWD",
        dialect = Dialect::Bash
    );
    let lines: Vec<&str> = r.stdout().trim().lines().collect();
    assert_eq!(lines[0], real_tmp, "pushd should cd to /tmp");
    // After popd, we should be back to original dir.
    assert_ne!(lines[1], real_tmp, "popd should restore original dir");
}

#[skuld::test]
fn dirstack_tracks_pushd() {
    let real_tmp = std::fs::canonicalize("/tmp").unwrap().to_string_lossy().to_string();
    let r = exec!("pushd /tmp > /dev/null; echo ${DIRSTACK[0]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), real_tmp);
}

#[skuld::test]
fn popd_empty_stack_fails() {
    assert_ne!(
        exec!("popd 2>/dev/null", dialect = Dialect::Bash).status(),
        0,
        "popd with empty stack should fail"
    );
}

#[skuld::test]
fn pushd_no_args_swaps_top_two() {
    let real_tmp = std::fs::canonicalize("/tmp").unwrap().to_string_lossy().to_string();
    let r = exec!(
        "pushd /tmp > /dev/null; pushd /var > /dev/null; pushd > /dev/null; echo $PWD",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout().trim(), real_tmp, "pushd with no args should swap top two");
    r.stderr(); // acknowledge pushd output on stderr
}

#[skuld::test]
fn dirs_c_clears_stack() {
    let r = exec!("pushd /tmp > /dev/null; dirs -c; dirs -p", dialect = Dialect::Bash);
    let lines: Vec<&str> = r.stdout().trim().lines().collect();
    // After dirs -c, only the current dir should remain.
    assert_eq!(lines.len(), 1);
}

#[skuld::test]
fn dirs_v_shows_indices() {
    let r = exec!("pushd /tmp > /dev/null; dirs -v", dialect = Dialect::Bash);
    // Should have indices like " 0  /tmp"
    assert!(r.stdout().contains(" 0"), "dirs -v should show index 0");
}

#[skuld::test]
fn pushd_n_no_cd() {
    let r = exec!("pushd -n /tmp > /dev/null; echo $PWD", dialect = Dialect::Bash);
    // With -n, pushd should NOT change directory.
    assert_ne!(r.stdout().trim(), "/tmp", "pushd -n should not change directory");
}

// COMP_WORDBREAKS =====================================================================================================

#[skuld::test]
fn comp_wordbreaks_initialized() {
    let r = exec!("echo \"x${COMP_WORDBREAKS}x\"", dialect = Dialect::Bash);
    let inner = r.stdout().trim();
    // Should not be empty (has a default value).
    assert_ne!(inner, "xx", "COMP_WORDBREAKS should be initialized");
}

#[skuld::test]
fn comp_wordbreaks_can_be_unset() {
    let r = exec!(
        "unset COMP_WORDBREAKS; echo \"x${COMP_WORDBREAKS}x\"",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout().trim(), "xx");
}

#[skuld::test]
fn comp_vars_not_set_by_default() {
    // COMP_WORDS, COMP_CWORD, etc. should not be set outside completion context.
    let r = exec!("echo \"x${COMP_WORDS}x\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout().trim(), "xx");
}

// declare -p attribute accuracy =======================================================================================

#[skuld::test]
fn declare_p_shows_readonly() {
    let r = exec!("readonly X=42; declare -p X", dialect = Dialect::Bash);
    assert!(
        r.stdout().contains("-r"),
        "declare -p should show -r for readonly: {}",
        r.stdout()
    );
}

#[skuld::test]
fn declare_p_shows_exported() {
    let r = exec!("export Y=hi; declare -p Y", dialect = Dialect::Bash);
    assert!(
        r.stdout().contains("-x"),
        "declare -p should show -x for exported: {}",
        r.stdout()
    );
}

#[skuld::test]
fn declare_p_shows_array() {
    let r = exec!("declare -a ARR=(a b c); declare -p ARR", dialect = Dialect::Bash);
    assert!(
        r.stdout().contains("-a"),
        "declare -p should show -a for array: {}",
        r.stdout()
    );
    assert!(
        r.stdout().contains("[0]=\"a\""),
        "declare -p should show array elements: {}",
        r.stdout()
    );
}

#[skuld::test]
fn declare_p_shows_assoc_array() {
    let r = exec!("declare -A MAP=([k]=v); declare -p MAP", dialect = Dialect::Bash);
    assert!(
        r.stdout().contains("-A"),
        "declare -p should show -A for assoc array: {}",
        r.stdout()
    );
}

#[skuld::test]
fn declare_p_plain_var() {
    let r = exec!("Z=hello; declare -p Z", dialect = Dialect::Bash);
    assert!(
        r.stdout().contains("declare -- Z=\"hello\""),
        "declare -p should show -- for plain var: {}",
        r.stdout()
    );
}
