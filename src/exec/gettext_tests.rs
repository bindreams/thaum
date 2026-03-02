use super::*;
use crate::exec::environment::Environment;

testutil::default_labels!(exec);

fn fixture_dir() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locale")
        .to_str()
        .unwrap()
        .to_string()
}

fn make_env_with_gettext() -> Environment {
    let mut env = Environment::new();
    let _ = env.set_var("TEXTDOMAINDIR", &fixture_dir());
    let _ = env.set_var("TEXTDOMAIN", "testdomain");
    let _ = env.set_var("LC_MESSAGES", "de");
    env
}

#[testutil::test]
fn translate_basic() {
    clear_cache();
    let env = make_env_with_gettext();
    assert_eq!(translate("hello world", &env), "hallo welt");
}

#[testutil::test]
fn translate_preserves_dollar_var() {
    clear_cache();
    let env = make_env_with_gettext();
    // Translation contains $USER literally -- NOT expanded here
    assert_eq!(translate("hello $USER", &env), "hallo $USER");
}

#[testutil::test]
fn translate_no_domain() {
    clear_cache();
    let env = Environment::new();
    // No TEXTDOMAIN set
    assert_eq!(translate("hello world", &env), "hello world");
}

#[testutil::test]
fn translate_missing_msgid() {
    clear_cache();
    let env = make_env_with_gettext();
    assert_eq!(translate("not in catalog", &env), "not in catalog");
}

#[testutil::test]
fn translate_empty() {
    clear_cache();
    let env = make_env_with_gettext();
    assert_eq!(translate("", &env), "");
}

#[testutil::test]
fn translate_c_locale() {
    clear_cache();
    let mut env = make_env_with_gettext();
    let _ = env.set_var("LC_MESSAGES", "C");
    assert_eq!(translate("hello world", &env), "hello world");
}

#[testutil::test]
fn translate_posix_locale() {
    clear_cache();
    let mut env = make_env_with_gettext();
    let _ = env.set_var("LC_MESSAGES", "POSIX");
    assert_eq!(translate("hello world", &env), "hello world");
}

#[testutil::test]
fn translate_missing_file() {
    clear_cache();
    let mut env = Environment::new();
    let _ = env.set_var("TEXTDOMAIN", "nonexistent");
    let _ = env.set_var("TEXTDOMAINDIR", "/tmp/nonexistent");
    let _ = env.set_var("LC_MESSAGES", "de");
    assert_eq!(translate("hello", &env), "hello");
}

#[testutil::test]
fn translate_caches_catalog() {
    clear_cache();
    let env = make_env_with_gettext();
    // First call loads the catalog
    assert_eq!(translate("hello world", &env), "hallo welt");
    // Second call uses cache
    assert_eq!(translate("goodbye", &env), "auf wiedersehen");
}

#[testutil::test]
fn translate_goodbye() {
    clear_cache();
    let env = make_env_with_gettext();
    assert_eq!(translate("goodbye", &env), "auf wiedersehen");
}

#[testutil::test]
fn translate_empty_domain() {
    clear_cache();
    let mut env = make_env_with_gettext();
    let _ = env.set_var("TEXTDOMAIN", "");
    assert_eq!(translate("hello world", &env), "hello world");
}

#[testutil::test]
fn translate_empty_textdomaindir() {
    clear_cache();
    let mut env = make_env_with_gettext();
    let _ = env.set_var("TEXTDOMAINDIR", "");
    assert_eq!(translate("hello world", &env), "hello world");
}

// locale_variants tests -------------------------------------------------------

#[testutil::test]
fn locale_variants_full() {
    let v = locale_variants("de_DE.UTF-8");
    assert!(v.contains(&"de_DE.UTF-8".to_string()));
    assert!(v.contains(&"de_DE".to_string()));
    assert!(v.contains(&"de".to_string()));
}

#[testutil::test]
fn locale_variants_no_charset() {
    let v = locale_variants("de_DE");
    assert!(v.contains(&"de_DE".to_string()));
    assert!(v.contains(&"de".to_string()));
}

#[testutil::test]
fn locale_variants_language_only() {
    let v = locale_variants("de");
    assert_eq!(v, vec!["de"]);
}

#[testutil::test]
fn default_textdomaindir_not_empty_on_unix() {
    if !cfg!(windows) {
        #[allow(clippy::const_is_empty)] // DEFAULT_TEXTDOMAINDIR resolved at build time
        {
            assert!(!DEFAULT_TEXTDOMAINDIR.is_empty());
        }
    }
}

// resolve_messages_locale tests -----------------------------------------------

#[testutil::test]
fn locale_lc_all_overrides_lc_messages() {
    let mut env = Environment::new();
    let _ = env.set_var("LC_ALL", "fr");
    let _ = env.set_var("LC_MESSAGES", "de");
    let _ = env.set_var("LANG", "en");
    assert_eq!(resolve_messages_locale(&env), "fr");
}

#[testutil::test]
fn locale_lc_messages_overrides_lang() {
    let mut env = Environment::new();
    let _ = env.set_var("LC_MESSAGES", "de");
    let _ = env.set_var("LANG", "en");
    assert_eq!(resolve_messages_locale(&env), "de");
}

#[testutil::test]
fn locale_falls_back_to_lang() {
    let mut env = Environment::new();
    let _ = env.set_var("LANG", "en");
    assert_eq!(resolve_messages_locale(&env), "en");
}

#[testutil::test]
fn locale_defaults_to_c() {
    let env = Environment::new();
    assert_eq!(resolve_messages_locale(&env), "C");
}
