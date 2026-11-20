//! Whether a text is a duration string, which Polars says.
//!
//! A duration string is whole amounts of units one after another, such as `1mo` or `2d12h`, which
//! the methods that move, cut and round a moment read. The catalogue does not parse one itself: it
//! asks Polars, by running the method the duration is given to on one moment, which refuses a text
//! that is no duration, one it reads and the method cannot use, as a negative one to cut by, and
//! says why. A text whose amounts cannot be held is refused before Polars is asked, for Polars adds
//! them up unchecked and a build with overflow checks panics where one without wraps around.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use polars::prelude::{DataType, IntoLazy, TimeUnit, col, df, lit};

/// The moment a duration is asked of when it is not known what it is given to: a `Datetime`.
#[must_use]
pub const fn any_moment() -> DataType {
    DataType::Datetime(TimeUnit::Microseconds, None)
}

/// Why a text is no duration, in the words Polars gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationError {
    reason: String,
}

impl DurationError {
    /// What Polars says of the text: what it expected and did not find.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for DurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for DurationError {}

/// The method a duration string is given to, which is asked whether it reads the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUse {
    /// `dt.offset_by`: the amount a moment is moved by.
    Offset,
    /// `dt.truncate`: the length of the periods a moment is cut to the start of.
    Cut,
    /// `dt.round`: the length of the periods a moment is rounded to the nearest of.
    Round,
}

/// Whether Polars reads `text` as a duration for the method `using` on a moment of the dtype
/// `on`, in a length it can hold: what it refuses depends on the dtype, as a `Date` cannot be cut
/// by a length of days and hours together.
///
/// # Errors
///
/// Polars' own reason, where the method refuses the text, or that it is too large to hold.
pub fn check_duration(text: &str, using: DurationUse, on: &DataType) -> Result<(), DurationError> {
    const TOO_LARGE: &str = "the duration is too large for Polars to hold";
    if !fits_polars(text) {
        return Err(DurationError {
            reason: TOO_LARGE.to_owned(),
        });
    }
    // What `fits_polars` lets through does not overflow Polars' parser; the catch is for a text
    // that Polars panics on, as an amount of the unit of an index that its rounding cannot divide
    // by.
    match catch_unwind(AssertUnwindSafe(|| ask_polars(text, using, on))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(reason)) => Err(DurationError { reason }),
        Err(_) => Err(DurationError {
            reason: "Polars fails when it uses this duration".to_owned(),
        }),
    }
}

/// What Polars says when `using` is run on one moment with the duration `text`, up to its first
/// blank line, which is where it starts to say which expression it was.
fn ask_polars(text: &str, using: DurationUse, on: &DataType) -> Result<(), String> {
    crate::startup::ensure();
    let moment = col("t");
    let asked = match using {
        DurationUse::Offset => moment.dt().offset_by(lit(text)),
        DurationUse::Cut => moment.dt().truncate(lit(text)),
        DurationUse::Round => moment.dt().round(lit(text)),
    };
    df!("t" => [0_i64])
        .map_err(|error| error.to_string())?
        .lazy()
        .select([col("t").cast(on.clone())])
        .select([asked.alias("asked")])
        .collect()
        .map(drop)
        .map_err(|error| {
            let said = error.to_string();
            said.split("\n\n").next().unwrap_or(&said).to_owned()
        })
}

/// The lengths of time Polars converts the parts of a duration with, in nanoseconds.
mod span {
    pub(super) const MICROSECOND: i64 = 1_000;
    pub(super) const MILLISECOND: i64 = 1_000 * MICROSECOND;
    pub(super) const SECOND: i64 = 1_000 * MILLISECOND;
    pub(super) const MINUTE: i64 = 60 * SECOND;
    pub(super) const HOUR: i64 = 60 * MINUTE;
    pub(super) const DAY: i64 = 24 * HOUR;
    pub(super) const WEEK: i64 = 7 * DAY;
    /// A month as Polars counts it in the length of a duration: 28 days.
    pub(super) const MONTH: i64 = 28 * DAY;
    /// The longest a month, a week and a day may be where Polars bounds a duration: 31 days and
    /// an hour, a week of six days and a day, and a day of 25 hours and a nanosecond.
    pub(super) const MONTH_AT_MOST: i64 = (31 * 24 + 1) * HOUR;
    pub(super) const DAY_AT_MOST: i64 = 25 * HOUR + 1;
    pub(super) const WEEK_AT_MOST: i64 = 6 * DAY + DAY_AT_MOST;
}

/// What each unit of a duration string counts, which is one of the parts Polars holds a
/// duration as: months, weeks, days and nanoseconds.
enum Count {
    Nanoseconds(i64),
    Days,
    Weeks,
    Months(i64),
}

/// How a unit of a duration string counts. `None` for a word that is no unit, which Polars refuses
/// with its own reason.
fn count_of(unit: &[u8]) -> Option<Count> {
    Some(match unit {
        b"ns" | b"i" => Count::Nanoseconds(1),
        b"us" => Count::Nanoseconds(span::MICROSECOND),
        b"ms" => Count::Nanoseconds(span::MILLISECOND),
        b"s" => Count::Nanoseconds(span::SECOND),
        b"m" => Count::Nanoseconds(span::MINUTE),
        b"h" => Count::Nanoseconds(span::HOUR),
        b"d" => Count::Days,
        b"w" => Count::Weeks,
        b"mo" => Count::Months(1),
        b"q" => Count::Months(3),
        b"y" => Count::Months(12),
        _ => return None,
    })
}

/// The parts of a duration, each an `i64` that holds what the string adds up to.
#[derive(Default)]
struct Parts {
    months: i64,
    weeks: i64,
    days: i64,
    nanoseconds: i64,
}

impl Parts {
    /// Adds `amount` of `count`, or `None` where a part overflows.
    fn add(&mut self, amount: i64, count: &Count) -> Option<()> {
        match count {
            Count::Nanoseconds(per) => {
                self.nanoseconds = self.nanoseconds.checked_add(amount.checked_mul(*per)?)?;
            }
            Count::Days => self.days = self.days.checked_add(amount)?,
            Count::Weeks => self.weeks = self.weeks.checked_add(amount)?,
            Count::Months(per) => {
                self.months = self.months.checked_add(amount.checked_mul(*per)?)?;
            }
        }
        Some(())
    }

    /// The nanoseconds the parts come to where a month, a week and a day are as long as given,
    /// or `None` where they do not fit an `i64`.
    fn nanoseconds_with(&self, month: i64, week: i64, day: i64) -> Option<i64> {
        self.months
            .checked_mul(month)?
            .checked_add(self.weeks.checked_mul(week)?)?
            .checked_add(self.days.checked_mul(day)?)?
            .checked_add(self.nanoseconds)
    }

    /// Whether the duration is one Polars holds: its length, where it adds the parts up with a
    /// month of 28 days, and the longest it may be, where a month is 31 days and an hour, a week
    /// 6 days and a day, and a day 25 hours and a nanosecond, each fit an `i64` of nanoseconds.
    fn fit(&self) -> bool {
        self.nanoseconds_with(span::MONTH, span::WEEK, span::DAY)
            .is_some()
            && self
                .nanoseconds_with(span::MONTH_AT_MOST, span::WEEK_AT_MOST, span::DAY_AT_MOST)
                .is_some()
    }
}

/// Whether the duration string `text` is one Polars holds. Polars adds the amounts up with
/// arithmetic that is not checked, so that a build with overflow checks panics and one without
/// wraps around, and it does not convert months, weeks and days into nanoseconds until the
/// duration is used, in its length (`duration_ns`) and in the longest it may be
/// (`nte_duration_ns`), as a window, a half-life, a literal and a join's tolerance do. This is
/// checked here, the same in every build, and only to find an amount that is too large: what is
/// no duration for another reason is Polars' to refuse.
fn fits_polars(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    let mut parts = Parts::default();
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        let mut amount = 0_i64;
        while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
            let Some(next) = amount
                .checked_mul(10)
                .and_then(|tens| tens.checked_add(i64::from(digit - b'0')))
            else {
                return false;
            };
            amount = next;
            at += 1;
        }
        let unit_start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
        }
        let Some(count) = count_of(&bytes[unit_start..at]) else {
            return true;
        };
        if parts.add(amount, &count).is_none() {
            return false;
        }
    }
    parts.fit()
}
