//! [`TimestampMs`]: millisecond time with a private RFC 3339 codec.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Latest representable time, `9999-12-31T23:59:59.999Z`.
const MAX_MS: u64 = 253_402_300_799_999;

/// A time at millisecond precision: the constructor truncates, so a row
/// equals `try_from_fields(to_fields(row))`. Written RFC 3339 UTC, ms
/// (`YYYY-MM-DDTHH:MM:SS.mmmZ`). A time before the Unix epoch is stored as
/// the epoch, and one after year 9999 as the last millisecond of 9999.
///
/// # Examples
///
/// ```
/// use std::time::{Duration, UNIX_EPOCH};
/// use sc_decision_schema::session_log::TimestampMs;
///
/// let ts = TimestampMs::from_system_time(UNIX_EPOCH + Duration::from_nanos(1_500_999_999));
/// assert_eq!(ts.as_system_time(), UNIX_EPOCH + Duration::from_millis(1_500));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMs(u64);

impl TimestampMs {
    /// Truncates `time` to milliseconds.
    #[must_use]
    pub fn from_system_time(time: SystemTime) -> Self {
        let elapsed = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(MAX_MS);
        Self(millis.min(MAX_MS))
    }

    /// The time as a [`SystemTime`].
    #[must_use]
    pub fn as_system_time(self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.0)
    }

    /// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    pub(crate) fn to_rfc3339(self) -> String {
        let millis = self.0 % 1000;
        let total_seconds = self.0 / 1000;
        let seconds_of_day = total_seconds % 86_400;
        let (year, month, day) =
            civil_from_days(i64::try_from(total_seconds / 86_400).unwrap_or(0));
        format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
            seconds_of_day / 3600,
            seconds_of_day % 3600 / 60,
            seconds_of_day % 60
        )
    }

    /// Parses exactly the [`Self::to_rfc3339`] form.
    pub(crate) fn parse_rfc3339(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let shape = b"0000-00-00T00:00:00.000Z";
        if bytes.len() != shape.len()
            || !bytes.iter().zip(shape).all(|(byte, pattern)| {
                if *pattern == b'0' {
                    byte.is_ascii_digit()
                } else {
                    byte == pattern
                }
            })
        {
            return None;
        }
        let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().ok();
        let year = i64::from(number(0..4)?);
        let (month, day) = (number(5..7)?, number(8..10)?);
        let (hour, minute, second, millis) = (
            number(11..13)?,
            number(14..16)?,
            number(17..19)?,
            number(20..23)?,
        );
        if !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        let days = u64::try_from(days_from_civil(year, month, day)).ok()?;
        let seconds =
            days * 86_400 + u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second);
        Some(Self(seconds * 1000 + u64::from(millis)))
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
