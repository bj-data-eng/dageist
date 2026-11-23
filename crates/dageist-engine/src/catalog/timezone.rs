//! Whether a text is a time zone, which Polars says.
//!
//! A time zone is a name of the time zone database, such as `Europe/Berlin` or `UTC`, or a fixed
//! offset such as `+01:00`. The catalogue does not read one itself: it asks Polars' own reading of a
//! time zone, the one the binding makes before it builds a method of a moment, which gives the
//! zone Polars holds, or the reason it is none, in its words.

use std::fmt;

use polars::prelude::{PlSmallStr, TimeZone};

/// Why a text is no time zone, in the words Polars gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZoneError {
    reason: String,
}

impl TimeZoneError {
    /// What Polars says of the text, up to its first blank line, which is where it starts to hint
    /// at what was meant and at how to turn the check off.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for TimeZoneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for TimeZoneError {}

/// The time zone Polars reads `text` as, or none where it reads it as no zone, as the empty text
/// is.
///
/// # Errors
///
/// Polars' own reason, where the text is no time zone.
pub fn check_time_zone(text: &str) -> Result<Option<TimeZone>, TimeZoneError> {
    TimeZone::opt_try_new(Some(PlSmallStr::from(text))).map_err(|error| {
        let said = error.to_string();
        TimeZoneError {
            reason: said.split("\n\n").next().unwrap_or(&said).to_owned(),
        }
    })
}
