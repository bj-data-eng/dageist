//! Figures written as text, for the view to show as they are.
//!
//! The view formats no number: a count, a size or a time is text from here,
//! written the same way wherever it is shown.

use std::time::Duration;

/// A whole number with a comma between each group of three digits, as in
/// `2,000,066`.
pub(crate) fn count(number: usize) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A count and what is counted, `1 row` or `2,552 rows`.
pub(crate) fn quantity(number: usize, one: &str, other: &str) -> String {
    format!(
        "{} {}",
        count(number),
        if number == 1 { one } else { other }
    )
}

/// `value` in units of `unit`, rounded half up to a tenth, as a number of tenths.
fn tenths(value: u128, unit: u128) -> u128 {
    value.saturating_mul(10).saturating_add(unit / 2) / unit
}

/// A number of tenths written with its decimal point, `382` as `38.2`.
fn with_decimal(tenths: u128) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// A size in bytes: whole bytes below a kilobyte of 1,024 bytes, kilobytes with
/// one decimal below a megabyte, and megabytes with one decimal above.
pub(crate) fn size(bytes: usize) -> String {
    const KILOBYTE: u128 = 1024;
    const MEGABYTE: u128 = 1024 * KILOBYTE;
    let bytes = u128::try_from(bytes).unwrap_or(u128::MAX);
    if bytes < KILOBYTE {
        return format!("{bytes} B");
    }
    // A size that rounds up to 1,024 KB is written as the megabyte it is.
    let kilobytes = tenths(bytes, KILOBYTE);
    if kilobytes < 10 * KILOBYTE {
        format!("{} KB", with_decimal(kilobytes))
    } else {
        format!("{} MB", with_decimal(tenths(bytes, MEGABYTE)))
    }
}

/// A time: microseconds below a millisecond, milliseconds with one decimal
/// below a second, and seconds with one decimal above.
pub(crate) fn elapsed(time: Duration) -> String {
    const MILLISECOND: u128 = 1000;
    const SECOND: u128 = 1000 * MILLISECOND;
    let micros = time.as_micros();
    if micros < MILLISECOND {
        return format!("{micros} µs");
    }
    // A time that rounds up to 1,000 ms is written as the second it is.
    let millis = tenths(micros, MILLISECOND);
    if millis < 10_000 {
        format!("{} ms", with_decimal(millis))
    } else {
        format!("{} s", with_decimal(tenths(micros, SECOND)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{count, elapsed, quantity, size};

    #[test]
    fn a_count_has_a_comma_between_groups_of_three_digits() {
        let written: Vec<String> = [0, 7, 999, 1000, 12_345, 123_456, 2_000_066, 1_234_567_890]
            .into_iter()
            .map(count)
            .collect();
        assert_eq!(
            written,
            [
                "0",
                "7",
                "999",
                "1,000",
                "12,345",
                "123,456",
                "2,000,066",
                "1,234,567,890"
            ]
        );
    }

    #[test]
    fn a_quantity_is_singular_only_for_one() {
        assert_eq!(quantity(0, "row", "rows"), "0 rows");
        assert_eq!(quantity(1, "row", "rows"), "1 row");
        assert_eq!(quantity(2, "row", "rows"), "2 rows");
        assert_eq!(quantity(2_552, "column", "columns"), "2,552 columns");
    }

    #[test]
    fn a_size_is_in_the_largest_unit_it_fills() {
        let written: Vec<String> = [
            0, 512, 1023, 1024, 1536, 39_128, 1_048_575, 1_048_576, 10_276_045,
        ]
        .into_iter()
        .map(size)
        .collect();
        assert_eq!(
            written,
            [
                "0 B", "512 B", "1023 B", "1.0 KB", "1.5 KB", "38.2 KB", "1.0 MB", "1.0 MB",
                "9.8 MB"
            ]
        );
    }

    #[test]
    fn a_time_is_in_the_largest_unit_it_fills() {
        let written: Vec<String> = [
            Duration::ZERO,
            Duration::from_micros(321),
            Duration::from_micros(1_500),
            Duration::from_micros(12_340),
            Duration::from_millis(412),
            Duration::from_millis(1_900),
            Duration::from_micros(999_960),
            Duration::from_secs(62),
        ]
        .into_iter()
        .map(elapsed)
        .collect();
        assert_eq!(
            written,
            [
                "0 µs", "321 µs", "1.5 ms", "12.3 ms", "412.0 ms", "1.9 s", "1.0 s", "62.0 s"
            ]
        );
    }
}
