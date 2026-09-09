use std::collections::HashMap;
use std::sync::OnceLock;

const SPANISH_COPY: &str = include_str!("../resources/es-AR.json");

pub(crate) fn text(key: &str) -> &'static str {
    static COPY: OnceLock<HashMap<String, String>> = OnceLock::new();
    let values = COPY.get_or_init(|| {
        serde_json::from_str(SPANISH_COPY)
            .expect("resources/es-AR.json debe contener un objeto de textos")
    });
    values.get(key).map_or_else(
        || panic!("falta el texto configurado: {key}"),
        String::as_str,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{SPANISH_COPY, text};

    #[test]
    fn embedded_copy_contains_the_product_title() {
        assert_eq!(text("app.title"), "Worklogger");
    }

    #[test]
    fn embedded_copy_has_no_empty_values() {
        let values: HashMap<String, String> =
            serde_json::from_str(SPANISH_COPY).expect("copy resource is valid JSON");
        assert!(values.values().all(|value| !value.trim().is_empty()));
    }
}
