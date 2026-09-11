#[cfg(any(feature = "hours", feature = "dev-desktop", test))]
use std::{collections::HashMap, sync::OnceLock};

#[cfg(any(feature = "hours", feature = "dev-desktop", test))]
const ENGLISH_COPY: &str = include_str!("../resources/en.json");
#[cfg(any(feature = "hours", feature = "dev-desktop", test))]
const SPANISH_COPY: &str = include_str!("../resources/es.json");

#[cfg(any(feature = "hours", feature = "dev-desktop", test))]
pub(crate) fn text(key: &str) -> &'static str {
    static COPY: OnceLock<HashMap<String, String>> = OnceLock::new();
    let values = COPY.get_or_init(|| {
        serde_json::from_str(selected_copy())
            .expect("the selected copy resource must contain a text object")
    });
    values.get(key).map_or_else(
        || panic!("falta el texto configurado: {key}"),
        String::as_str,
    )
}

#[cfg(any(feature = "hours", feature = "dev-desktop", test))]
fn selected_copy() -> &'static str {
    match preferred_language() {
        worklogger_settings::Language::English => ENGLISH_COPY,
        worklogger_settings::Language::Spanish => SPANISH_COPY,
    }
}

pub(crate) fn preferred_language() -> worklogger_settings::Language {
    worklogger_settings::SettingsStore::for_current_user()
        .ok()
        .and_then(|store| store.load().ok().flatten())
        .and_then(|settings| settings.language)
        .unwrap_or_default()
}

#[cfg(any(feature = "hours", feature = "dev-desktop"))]
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn save_language(language: worklogger_settings::Language) -> Result<(), String> {
    let store = worklogger_settings::SettingsStore::for_current_user()
        .map_err(|error| error.to_string())?;
    let mut settings = store
        .load()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    settings.language = Some(language);
    store
        .save(&settings, settings.revision)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ENGLISH_COPY, SPANISH_COPY, text};

    #[test]
    fn embedded_copy_contains_the_product_title() {
        assert_eq!(text("app.title"), "Worklogger");
    }

    #[test]
    fn embedded_copy_has_no_empty_values() {
        let values: HashMap<String, String> =
            serde_json::from_str(ENGLISH_COPY).expect("English copy resource is valid JSON");
        assert!(values.values().all(|value| !value.trim().is_empty()));
    }

    /// Compares the key sets rather than their sizes.
    ///
    /// `text` panics on a key it cannot find, so a key present in only one
    /// language crashes the window for whoever selected that language. Equal
    /// counts do not rule that out: two files can hold the same number of
    /// different keys.
    #[test]
    fn both_languages_define_exactly_the_same_keys() {
        let english: HashMap<String, String> =
            serde_json::from_str(ENGLISH_COPY).expect("English copy resource is valid JSON");
        let spanish: HashMap<String, String> =
            serde_json::from_str(SPANISH_COPY).expect("Spanish copy resource is valid JSON");

        let mut only_english: Vec<&str> = english
            .keys()
            .filter(|key| !spanish.contains_key(*key))
            .map(String::as_str)
            .collect();
        let mut only_spanish: Vec<&str> = spanish
            .keys()
            .filter(|key| !english.contains_key(*key))
            .map(String::as_str)
            .collect();
        only_english.sort_unstable();
        only_spanish.sort_unstable();

        assert!(
            only_english.is_empty() && only_spanish.is_empty(),
            "missing translations -> only in English: {only_english:?}; only in Spanish: {only_spanish:?}"
        );
        assert!(spanish.values().all(|value| !value.trim().is_empty()));
    }

    /// Guards the key the header needs, without assuming a language.
    ///
    /// `text` resolves against whichever language is selected, so asserting an
    /// English string here would fail on a Spanish machine.
    #[test]
    fn the_account_selector_has_copy_in_both_languages() {
        for resource in [ENGLISH_COPY, SPANISH_COPY] {
            let values: HashMap<String, String> =
                serde_json::from_str(resource).expect("copy resource is valid JSON");
            assert!(values.contains_key("action.selectAccount"));
        }
    }
}
