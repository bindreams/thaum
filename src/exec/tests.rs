//! Executor unit tests: pattern matching, word expansion, builtins, variables,
//! control flow, and compound command execution.

use super::*;
use pattern::{trim_largest_prefix, trim_largest_suffix, trim_smallest_prefix, trim_smallest_suffix};

#[test]
fn pattern_match_literal() {
    assert!(shell_pattern_match("hello", "hello"));
    assert!(!shell_pattern_match("hello", "world"));
}

#[test]
fn pattern_match_star() {
    assert!(shell_pattern_match("hello", "*"));
    assert!(shell_pattern_match("hello", "hel*"));
    assert!(shell_pattern_match("hello", "*llo"));
    assert!(shell_pattern_match("hello", "h*o"));
    assert!(!shell_pattern_match("hello", "h*x"));
}

#[test]
fn pattern_match_question() {
    assert!(shell_pattern_match("hello", "hell?"));
    assert!(shell_pattern_match("hello", "?ello"));
    assert!(!shell_pattern_match("hello", "hell"));
}

#[test]
fn pattern_match_bracket() {
    assert!(shell_pattern_match("hello", "[h]ello"));
    assert!(shell_pattern_match("hello", "[a-z]ello"));
    assert!(!shell_pattern_match("hello", "[A-Z]ello"));
    assert!(shell_pattern_match("hello", "[!A-Z]ello"));
}

// Trim prefix/suffix tests --------------------------------------------------------------------------------------------

#[test]
fn trim_smallest_prefix_star_slash() {
    // ${var#*/} — remove shortest prefix ending in /
    assert_eq!(
        trim_smallest_prefix("/usr/bin:/usr/local/bin", "*/"),
        "usr/bin:/usr/local/bin"
    );
}

#[test]
fn trim_largest_prefix_star_slash() {
    // ${var##*/} — remove longest prefix ending in /
    assert_eq!(trim_largest_prefix("/usr/bin:/usr/local/bin", "*/"), "bin");
}

#[test]
fn trim_smallest_suffix_dot_star() {
    // ${var%.*} — remove shortest suffix starting with .
    assert_eq!(trim_smallest_suffix("archive.tar.gz", ".*"), "archive.tar");
}

#[test]
fn trim_largest_suffix_dot_star() {
    // ${var%%.*} — remove longest suffix starting with .
    assert_eq!(trim_largest_suffix("archive.tar.gz", ".*"), "archive");
}

#[test]
fn trim_no_match_returns_original() {
    assert_eq!(trim_smallest_prefix("hello", "xyz"), "hello");
    assert_eq!(trim_largest_prefix("hello", "xyz"), "hello");
    assert_eq!(trim_smallest_suffix("hello", "xyz"), "hello");
    assert_eq!(trim_largest_suffix("hello", "xyz"), "hello");
}

#[test]
fn trim_empty_pattern_matches_empty_string() {
    // Empty pattern matches empty string, which is a prefix/suffix of everything
    assert_eq!(trim_smallest_prefix("hello", ""), "hello");
    assert_eq!(trim_largest_prefix("hello", ""), "hello");
    assert_eq!(trim_smallest_suffix("hello", ""), "hello");
    assert_eq!(trim_largest_suffix("hello", ""), "hello");
}

#[test]
fn trim_prefix_basename() {
    // Common idiom: ${path##*/} extracts basename
    assert_eq!(trim_largest_prefix("/a/b/c.txt", "*/"), "c.txt");
}

#[test]
fn trim_suffix_extension() {
    // Common idiom: ${file%.*} removes extension
    assert_eq!(trim_smallest_suffix("file.txt", ".*"), "file");
}

#[test]
fn trim_suffix_dirname() {
    // ${path%/*} extracts dirname
    assert_eq!(trim_smallest_suffix("/a/b/c.txt", "/*"), "/a/b");
}
