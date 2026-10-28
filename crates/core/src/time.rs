// SPDX-License-Identifier: Apache-2.0
//! Calendar formatting without a date/time library dependency.
//!
//! All conversions are integer-only and work for pre-1970 values. Timestamps
//! are rendered as UTC when the Parquet logical type says they are adjusted to
//! UTC; unadjusted values carry no `Z` suffix and are labelled by the caller.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeUnit {
    Millis,
    Micros,
    Nanos,
}

impl TimeUnit {
    fn per_second(self) -> i64 {
        match self {
            TimeUnit::Millis => 1_000,
            TimeUnit::Micros => 1_000_000,
            TimeUnit::Nanos => 1_000_000_000,
        }
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

pub fn format_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Render seconds since the epoch plus a nanosecond fraction.
pub fn format_timestamp_parts(seconds: i64, nanos: u32, adjusted_to_utc: bool) -> String {
    let days = seconds.div_euclid(86_400);
    let sod = seconds.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let hour = sod / 3_600;
    let minute = (sod % 3_600) / 60;
    let second = sod % 60;
    let mut out = format!("{y:04}-{mo:02}-{d:02}T{hour:02}:{minute:02}:{second:02}");
    if nanos > 0 {
        let mut fraction = format!("{nanos:09}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        out.push('.');
        out.push_str(&fraction);
    }
    if adjusted_to_utc {
        out.push('Z');
    }
    out
}

pub fn format_timestamp(value: i64, unit: TimeUnit, adjusted_to_utc: bool) -> String {
    let per_second = unit.per_second();
    let seconds = value.div_euclid(per_second);
    let remainder = value.rem_euclid(per_second);
    let nanos = match unit {
        TimeUnit::Millis => (remainder * 1_000_000) as u32,
        TimeUnit::Micros => (remainder * 1_000) as u32,
        TimeUnit::Nanos => remainder as u32,
    };
    format_timestamp_parts(seconds, nanos, adjusted_to_utc)
}

/// Render a time-of-day value as `HH:MM:SS[.fraction]`.
pub fn format_time_of_day(value: i64, unit: TimeUnit) -> String {
    let per_second = unit.per_second();
    let seconds = value.div_euclid(per_second).rem_euclid(86_400);
    let remainder = value.rem_euclid(per_second);
    let nanos = match unit {
        TimeUnit::Millis => (remainder * 1_000_000) as u32,
        TimeUnit::Micros => (remainder * 1_000) as u32,
        TimeUnit::Nanos => remainder as u32,
    };
    let mut out = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    );
    if nanos > 0 {
        let mut fraction = format!("{nanos:09}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        out.push('.');
        out.push_str(&fraction);
    }
    out
}

/// Format an integer with no locale-sensitive thousands separators.
pub fn format_count(value: u64) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn leap_day_and_negative_days() {
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(format_date(-1), "1969-12-31");
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }

    #[test]
    fn timestamp_units() {
        assert_eq!(
            format_timestamp(0, TimeUnit::Millis, true),
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            format_timestamp(1_500, TimeUnit::Millis, true),
            "1970-01-01T00:00:01.5Z"
        );
        assert_eq!(
            format_timestamp(1_500_000, TimeUnit::Micros, true),
            "1970-01-01T00:00:01.5Z"
        );
        assert_eq!(
            format_timestamp(1_500_000_000, TimeUnit::Nanos, true),
            "1970-01-01T00:00:01.5Z"
        );
    }

    #[test]
    fn pre_epoch_with_fraction() {
        assert_eq!(
            format_timestamp(-1, TimeUnit::Millis, true),
            "1969-12-31T23:59:59.999Z"
        );
        assert_eq!(
            format_timestamp(-1_500, TimeUnit::Millis, true),
            "1969-12-31T23:59:58.5Z"
        );
        assert_eq!(
            format_timestamp(0, TimeUnit::Millis, false),
            "1970-01-01T00:00:00"
        );
    }

    #[test]
    fn time_of_day_wraps() {
        assert_eq!(format_time_of_day(0, TimeUnit::Millis), "00:00:00");
        assert_eq!(
            format_time_of_day(3_600_000 + 61_500, TimeUnit::Millis),
            "01:01:01.5"
        );
    }

    #[test]
    fn known_far_future() {
        let days = days_from_civil(2100, 3, 1);
        assert_eq!(civil_from_days(days), (2100, 3, 1));
        assert_eq!(format_date(days), "2100-03-01");
    }
}
