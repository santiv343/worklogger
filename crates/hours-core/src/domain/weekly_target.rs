use serde::{Deserialize, Serialize};

use super::{Duration, HoursError};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeeklyTarget(Duration);

impl WeeklyTarget {
    /// Creates a positive weekly target.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidWeeklyTarget`] when `minutes` is invalid.
    pub fn from_minutes(minutes: u32) -> Result<Self, HoursError> {
        Duration::from_minutes(minutes)
            .map(Self)
            .map_err(|_| HoursError::InvalidWeeklyTarget)
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}
