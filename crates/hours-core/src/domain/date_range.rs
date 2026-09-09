use serde::{Deserialize, Serialize};
use time::{Date, Duration as TimeDuration, Weekday};

use super::HoursError;

const DAYS_AFTER_MONDAY: i64 = 6;
const INCLUSIVE_DAY_COUNT_ADJUSTMENT: u64 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DateRange {
    start: Date,
    end: Date,
}

impl DateRange {
    /// Creates an inclusive date range.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidDateRange`] when `start` is after `end`.
    pub fn new(start: Date, end: Date) -> Result<Self, HoursError> {
        if start > end {
            return Err(HoursError::InvalidDateRange);
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub fn week_containing(date: Date) -> Self {
        let days_from_monday = i64::from(date.weekday().number_days_from_monday());
        let start = date - TimeDuration::days(days_from_monday);
        let end = start + TimeDuration::days(DAYS_AFTER_MONDAY);
        Self { start, end }
    }

    #[must_use]
    pub const fn start(self) -> Date {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> Date {
        self.end
    }

    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        date >= self.start && date <= self.end
    }

    #[must_use]
    pub fn day_count(self) -> u64 {
        let distance = (self.end - self.start).whole_days();
        distance.unsigned_abs() + INCLUSIVE_DAY_COUNT_ADJUSTMENT
    }

    pub fn weekday_dates(self) -> impl Iterator<Item = (Weekday, Date)> {
        let last_day_offset = (self.end - self.start).whole_days();
        (0..=last_day_offset).map(move |day_offset| {
            let date = self.start + TimeDuration::days(day_offset);
            (date.weekday(), date)
        })
    }
}
