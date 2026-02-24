//! GNU gettext message catalog lookup for `$"..."` locale translation.
//!
//! Reads `.mo` files based on `$TEXTDOMAIN`, `$TEXTDOMAINDIR`, and the
//! `LC_MESSAGES` locale. Falls back to the untranslated string if no
//! catalog is found or the msgid has no translation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::exec::environment::Environment;

/// Default system locale directory. Overridable at build time:
///   `THAUM_TEXTDOMAINDIR=/custom/path cargo build`
const DEFAULT_TEXTDOMAINDIR: &str = match option_env!("THAUM_TEXTDOMAINDIR") {
    Some(dir) => dir,
    None => {
        if cfg!(windows) {
            ""
        } else {
            "/usr/share/locale"
        }
    }
};

thread_local! {
    static CATALOG_CACHE: RefCell<HashMap<(String, String), Option<gettext::Catalog>>> =
        RefCell::new(HashMap::new());
}

/// Look up a translation for `msgid` using the current `TEXTDOMAIN` and locale.
///
/// Returns the translated string if found, or `msgid` unchanged if:
/// - `TEXTDOMAIN` is not set or empty
/// - The locale is `C` or `POSIX`
/// - The `.mo` file does not exist
/// - The `msgid` has no translation in the catalog
pub fn translate(msgid: &str, env: &Environment) -> String {
    if msgid.is_empty() {
        return String::new();
    }

    let domain = match env.get_var("TEXTDOMAIN") {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => return msgid.to_string(),
    };

    let locale_str = resolve_messages_locale(env);
    if locale_str == "C" || locale_str == "POSIX" || locale_str.is_empty() {
        return msgid.to_string();
    }

    let base_dir = env
        .get_var("TEXTDOMAINDIR")
        .unwrap_or(DEFAULT_TEXTDOMAINDIR)
        .to_string();
    if base_dir.is_empty() {
        return msgid.to_string();
    }

    CATALOG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (domain.clone(), locale_str.clone());

        if !cache.contains_key(&key) {
            let catalog = try_load_catalog(&domain, &locale_str, &base_dir);
            cache.insert(key.clone(), catalog);
        }

        match cache.get(&key) {
            Some(Some(cat)) => cat.gettext(msgid).to_string(),
            _ => msgid.to_string(),
        }
    })
}

/// Clear the thread-local catalog cache. Useful for testing.
#[cfg(test)]
pub(crate) fn clear_cache() {
    CATALOG_CACHE.with(|cache| cache.borrow_mut().clear());
}

fn resolve_messages_locale(env: &Environment) -> String {
    env.get_var("LC_ALL")
        .or_else(|| env.get_var("LC_MESSAGES"))
        .or_else(|| env.get_var("LANG"))
        .unwrap_or("C")
        .to_string()
}

fn try_load_catalog(domain: &str, locale: &str, base_dir: &str) -> Option<gettext::Catalog> {
    for variant in locale_variants(locale) {
        let path = PathBuf::from(base_dir)
            .join(&variant)
            .join("LC_MESSAGES")
            .join(format!("{domain}.mo"));
        if let Ok(file) = File::open(&path) {
            if let Ok(catalog) = gettext::Catalog::parse(BufReader::new(file)) {
                return Some(catalog);
            }
        }
    }
    None
}

/// Generate locale directory variants to try, from most to least specific.
///
/// `"de_DE.UTF-8"` produces `["de_DE.UTF-8", "de_DE.utf-8", "de_DE", "de.UTF-8", "de.utf-8", "de"]`.
fn locale_variants(locale: &str) -> Vec<String> {
    let mut variants = Vec::new();
    variants.push(locale.to_string());

    // Normalize charset case: .UTF-8 -> .utf-8
    if let Some(dot) = locale.find('.') {
        let normalized = format!("{}.{}", &locale[..dot], locale[dot + 1..].to_lowercase());
        if normalized != locale {
            variants.push(normalized);
        }
    }

    // Strip charset
    if let Some(dot) = locale.find('.') {
        let without_charset = &locale[..dot];
        variants.push(without_charset.to_string());
    }

    // Strip country (de_DE -> de), also with charset variants
    if let Some(underscore) = locale.find('_') {
        let lang = &locale[..underscore];
        if let Some(dot) = locale.find('.') {
            let charset = &locale[dot..];
            variants.push(format!("{lang}{charset}"));
            variants.push(format!("{lang}.{}", locale[dot + 1..].to_lowercase()));
        }
        variants.push(lang.to_string());
    }

    variants.dedup();
    variants
}

#[cfg(test)]
#[path = "gettext_tests.rs"]
mod tests;
