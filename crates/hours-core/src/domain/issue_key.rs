use serde::{Deserialize, Serialize};

use super::HoursError;

const KEY_SEPARATOR: char = '-';

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct IssueKey(String);

impl IssueKey {
    /// Creates an uppercase Jira issue key.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidIssueKey`] when the value is malformed.
    pub fn new(value: impl Into<String>) -> Result<Self, HoursError> {
        let normalized = value.into().trim().to_ascii_uppercase();
        if !is_valid_key(&normalized) {
            return Err(HoursError::InvalidIssueKey);
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_key(value: &str) -> bool {
    let Some((project, number)) = value.split_once(KEY_SEPARATOR) else {
        return false;
    };
    is_valid_project(project) && is_valid_number(number)
}

fn is_valid_project(value: &str) -> bool {
    !value.is_empty()
        && value.chars().next().is_some_and(char::is_alphabetic)
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn is_valid_number(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}
