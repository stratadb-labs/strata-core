//! Audit test for issue #868: Incorrect date formatting in metadata fields
//! Verdict: CONFIRMED BUG
//!
//! The format_micros() function in crates/engine/src/bundle.rs uses a naive
//! date calculation that ignores leap years and assumes all months are 30 days.
//! This produces incorrect ISO 8601 date strings.

/// Reproduce the buggy format_micros function from crates/engine/src/bundle.rs:276-293
fn format_micros_buggy(micros: u64) -> String {
    let secs = micros / 1_000_000;
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let years = 1970 + (days / 365); // BUG: ignores leap years
    let day_of_year = days % 365; // BUG: wrong for leap years
    let month = (day_of_year / 30).min(11) + 1; // BUG: months are not 30 days
    let day = (day_of_year % 30) + 1; // BUG: wraps incorrectly

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        years, month, day, hours, minutes, seconds
    )
}

#[test]
fn issue_868_leap_year_drift() {
    // 2024-01-01 00:00:00 UTC = 19723 days since epoch
    // (1970 to 2024 = 54 years, with 13 leap years: 54*365 + 13 = 19723)
    let days_to_2024 = 19723u64;
    let micros_2024_jan_1 = days_to_2024 * 86400 * 1_000_000;

    let result = format_micros_buggy(micros_2024_jan_1);

    // The buggy function computes: years = 1970 + (19723 / 365) = 1970 + 54 = 2024
    // BUT day_of_year = 19723 % 365 = 13 (not 0!)
    // Because 54*365 = 19710, and 19723 - 19710 = 13
    // So it shows 2024-01-14 instead of 2024-01-01
    assert_ne!(
        &result[..10],
        "2024-01-01",
        "BUG: format_micros produces wrong date for 2024-01-01 due to leap year accumulation"
    );
    // The function drifts by ~13 days for dates in 2024 due to unaccounted leap years
}

#[test]
fn issue_868_month_length_error() {
    // February has 28-29 days, not 30. Test a date in March.
    // 2023-03-01 = (2023-1970)*365 + 13 leap days + 31 (Jan) + 28 (Feb) = 19352 days
    // But we can also compute directly:
    // The function treats all months as 30 days.
    // day_of_year = 59 (Jan=31 + Feb=28) for a non-leap year
    // month = (59/30) + 1 = 2 + 1 = 3 (March - happens to be correct!)
    // day = (59%30) + 1 = 29 + 1 = 30 (shows March 30 instead of March 1!)
    //
    // Let's test December 31 instead, where the error is more dramatic:
    // Dec 31 = day_of_year 364 (0-indexed)
    // month = (364/30).min(11) + 1 = 12.min(11) + 1 = 12
    // day = (364%30) + 1 = 4 + 1 = 5 (shows December 5 instead of December 31!)

    // For simplicity, test with a known timestamp
    // 2020-12-31 00:00:00 UTC
    let days_to_2020_dec_31 = 18627u64; // days since epoch
    let micros = days_to_2020_dec_31 * 86400 * 1_000_000;
    let result = format_micros_buggy(micros);

    // Due to both leap year drift and month-length errors,
    // the output will NOT be 2020-12-31
    // The function divides 365-day years and 30-day months naively
    assert!(
        !result.starts_with("2020-12-31"),
        "BUG: format_micros should produce incorrect date for Dec 31 due to naive month calculation. Got: {}",
        result
    );
}

#[test]
fn issue_868_epoch_is_correct() {
    // The only date that should be correct is the Unix epoch itself
    let result = format_micros_buggy(0);
    assert_eq!(
        result, "1970-01-01T00:00:00Z",
        "Epoch should format correctly"
    );
}

#[test]
fn issue_868_time_of_day_is_correct() {
    // Time-of-day computation is correct (hours, minutes, seconds)
    // It's only the date part that's broken
    // 12:34:56 on day 0 = 12*3600 + 34*60 + 56 = 45296 seconds
    let micros = 45296u64 * 1_000_000;
    let result = format_micros_buggy(micros);
    assert!(
        result.ends_with("T12:34:56Z"),
        "Time-of-day should be correct. Got: {}",
        result
    );
}
