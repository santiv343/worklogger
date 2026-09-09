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
        let spanish: HashMap<String, String> =
            serde_json::from_str(SPANISH_COPY).expect("Spanish copy resource is valid JSON");
        assert!(values.values().all(|value| !value.trim().is_empty()));
        assert_eq!(values.len(), spanish.len());
    }
}
