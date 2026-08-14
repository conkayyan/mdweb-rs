//! Date and calendar arithmetic shared by the template engine, content
//! loader, RSS feed and renderer. Everything works in the proleptic
//! Gregorian calendar and is std-only.

/// Whether `y` is a leap year.
pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Number of days in month `m` (1-12) of year `y`.
pub fn month_len(y: i64, m: u32) -> u32 {
    match m {
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}

/// Total number of days in year `y`.
pub fn days_in_year(y: i64) -> u32 {
    if is_leap(y) {
        366
    } else {
        365
    }
}

/// Weekday for a date; 0 = Sunday .. 6 = Saturday (Sakamoto's algorithm).
pub fn weekday(y: i64, m: u32, d: u32) -> usize {
    let t = [0i64, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (y + y / 4 - y / 100 + y / 400 + t[(m - 1) as usize] + i64::from(d)).rem_euclid(7) as usize
}

/// Days since the Unix epoch (`1970-01-01`) for a proleptic Gregorian date.
///
/// Uses the January-anchored year directly (not the March-adjusted one), so
/// Jan/Feb dates get the same year offset as every other month.
pub fn date_to_epoch(y: i64, m: u32, d: u32) -> i64 {
    let mut days = 365 * (y - 1970);
    days += (y - 1969) / 4;
    days -= (y - 1901) / 100;
    days += (y - 1601) / 400;
    let leap = if is_leap(y) && m > 2 { 1 } else { 0 };
    let md: i64 = match m {
        1 => 0,
        2 => 31,
        3 => 59,
        4 => 90,
        5 => 120,
        6 => 151,
        7 => 181,
        8 => 212,
        9 => 243,
        10 => 273,
        11 => 304,
        12 => 334,
        _ => 0,
    };
    days + md + leap + i64::from(d) - 1
}

/// The proleptic Gregorian year containing the given days-since-epoch.
pub fn year_for_days(days: i64) -> i64 {
    fn leaps(v: i64) -> i64 {
        v / 4 - v / 100 + v / 400
    }
    fn start(y: i64) -> i64 {
        365 * (y - 1970) + leaps(y - 1) - leaps(1969)
    }
    let mut y = 1970 + days / 366;
    if days < start(y) {
        while days < start(y) {
            y -= 1;
        }
    } else {
        while days >= start(y + 1) {
            y += 1;
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_year_rules() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(is_leap(2024));
        assert!(!is_leap(2025));
    }

    #[test]
    fn month_lengths() {
        assert_eq!(month_len(2024, 2), 29);
        assert_eq!(month_len(2025, 2), 28);
        assert_eq!(month_len(2025, 4), 30);
        assert_eq!(month_len(2025, 1), 31);
    }

    #[test]
    fn epoch_round_trip() {
        assert_eq!(date_to_epoch(1970, 1, 1), 0);
        assert_eq!(date_to_epoch(1970, 1, 2), 1);
        assert_eq!(date_to_epoch(1971, 1, 1), 365);
        assert_eq!(date_to_epoch(1972, 3, 1), 790); // leap year included
    }

    #[test]
    fn year_for_days_matches_epoch() {
        assert_eq!(year_for_days(0), 1970);
        assert_eq!(year_for_days(date_to_epoch(2026, 1, 1)), 2026);
        assert_eq!(year_for_days(date_to_epoch(1999, 12, 31)), 1999);
    }

    #[test]
    fn weekday_knows_saturdays() {
        assert_eq!(weekday(2026, 8, 14), 5); // Friday
        assert_eq!(weekday(2026, 8, 15), 6); // Saturday
        assert_eq!(weekday(2026, 8, 16), 0); // Sunday
    }
}
