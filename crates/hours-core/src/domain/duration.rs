use serde::{Deserialize, Serialize};

use super::HoursError;

const SECONDS_PER_MINUTE: u32 = 60;
const SECONDS_PER_HOUR: f64 = 3_600.0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Duration(u32);

impl Duration {
    /// Creates a positive duration from whole minutes.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidDuration`] when `minutes` is zero.
    pub fn from_minutes(minutes: u32) -> Result<Self, HoursError> {
        if minutes == 0 {
            return Err(HoursError::InvalidDuration);
        }
        let seconds = minutes
            .checked_mul(SECONDS_PER_MINUTE)
            .ok_or(HoursError::InvalidDuration)?;
        Ok(Self(seconds))
    }

    /// Creates a positive duration from whole seconds.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidDuration`] when `seconds` is zero.
    pub fn from_seconds(seconds: u32) -> Result<Self, HoursError> {
        if seconds == 0 {
            return Err(HoursError::InvalidDuration);
        }
        Ok(Self(seconds))
    }

    #[must_use]
    pub const fn seconds(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn hours(self) -> f64 {
        f64::from(self.0) / SECONDS_PER_HOUR
    }
}
