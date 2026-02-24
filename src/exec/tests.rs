//! Executor unit tests: pattern matching, word expansion, builtins, variables,
//! control flow, and compound command execution.

use super::*;
use icu::locale::Locale;
use pattern::{trim_largest_prefix, trim_largest_suffix, trim_smallest_prefix, trim_smallest_suffix};

fn c() -> Locale {
    locale::parse_posix_locale("C")
}

#[test]
fn pattern_match_literal() {
    let l = c();
    assert!(shell_pattern_match("hello", "hello", &l));
    assert!(!shell_pattern_match("hello", "world", &l));
}

#[test]
fn pattern_match_star() {
    let l = c();
    assert!(shell_pattern_match("hello", "*", &l));
    assert!(shell_pattern_match("hello", "hel*", &l));
    assert!(shell_pattern_match("hello", "*llo", &l));
    assert!(shell_pattern_match("hello", "h*o", &l));
    assert!(!shell_pattern_match("hello", "h*x", &l));
}

#[test]
fn pattern_match_question() {
    let l = c();
    assert!(shell_pattern_match("hello", "hell?", &l));
    assert!(shell_pattern_match("hello", "?ello", &l));
    assert!(!shell_pattern_match("hello", "hell", &l));
}

#[test]
fn pattern_match_bracket() {
    let l = c();
    assert!(shell_pattern_match("hello", "[h]ello", &l));
    assert!(shell_pattern_match("hello", "[a-z]ello", &l));
    assert!(!shell_pattern_match("hello", "[A-Z]ello", &l));
    assert!(shell_pattern_match("hello", "[!A-Z]ello", &l));
}

// Trim prefix/suffix tests --------------------------------------------------------------------------------------------

#[test]
fn trim_smallest_prefix_star_slash() {
    let l = c();
    // ${var#*/} — remove shortest prefix ending in /
    assert_eq!(
        trim_smallest_prefix("/usr/bin:/usr/local/bin", "*/", &l),
        "usr/bin:/usr/local/bin"
    );
}

#[test]
fn trim_largest_prefix_star_slash() {
    let l = c();
    // ${var##*/} — remove longest prefix ending in /
    assert_eq!(trim_largest_prefix("/usr/bin:/usr/local/bin", "*/", &l), "bin");
}

#[test]
fn trim_smallest_suffix_dot_star() {
    let l = c();
    // ${var%.*} — remove shortest suffix starting with .
    assert_eq!(trim_smallest_suffix("archive.tar.gz", ".*", &l), "archive.tar");
}

#[test]
fn trim_largest_suffix_dot_star() {
    let l = c();
    // ${var%%.*} — remove longest suffix starting with .
    assert_eq!(trim_largest_suffix("archive.tar.gz", ".*", &l), "archive");
}

#[test]
fn trim_no_match_returns_original() {
    let l = c();
    assert_eq!(trim_smallest_prefix("hello", "xyz", &l), "hello");
    assert_eq!(trim_largest_prefix("hello", "xyz", &l), "hello");
    assert_eq!(trim_smallest_suffix("hello", "xyz", &l), "hello");
    assert_eq!(trim_largest_suffix("hello", "xyz", &l), "hello");
}

#[test]
fn trim_empty_pattern_matches_empty_string() {
    let l = c();
    // Empty pattern matches empty string, which is a prefix/suffix of everything
    assert_eq!(trim_smallest_prefix("hello", "", &l), "hello");
    assert_eq!(trim_largest_prefix("hello", "", &l), "hello");
    assert_eq!(trim_smallest_suffix("hello", "", &l), "hello");
    assert_eq!(trim_largest_suffix("hello", "", &l), "hello");
}

#[test]
fn trim_prefix_basename() {
    let l = c();
    // Common idiom: ${path##*/} extracts basename
    assert_eq!(trim_largest_prefix("/a/b/c.txt", "*/", &l), "c.txt");
}

#[test]
fn trim_suffix_extension() {
    let l = c();
    // Common idiom: ${file%.*} removes extension
    assert_eq!(trim_smallest_suffix("file.txt", ".*", &l), "file");
}

#[test]
fn trim_suffix_dirname() {
    let l = c();
    // ${path%/*} extracts dirname
    assert_eq!(trim_smallest_suffix("/a/b/c.txt", "/*", &l), "/a/b");
}
