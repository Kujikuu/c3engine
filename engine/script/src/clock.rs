//! Time windows scripts check (`time_within(mode, spec)`), in local time, both ends
//! included: 0 a span of dates `Y-M-D h:m Y-M-D h:m`, 1 yearly `M-D h:m M-D h:m`, 2 monthly
//! `day h:m day h:m`, 3 weekly `weekday h:m weekday h:m` (Sunday is 0, or 7 to end a week
//! that starts on Monday), 4 daily `h:m h:m`,
//! 5 hourly `minute minute`. A window doesn't wrap: its start comes before its end.

use chrono::{Datelike, Local, NaiveDateTime, Timelike};

/// The local time now.
pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// The numbers of `spec`, separated by spaces, `-` and `:`.
fn numbers(spec: &str) -> Option<Vec<i64>> {
    (spec.split([' ', '-', ':']))
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect()
}

/// Whether `at` is in the window `spec` of `mode`; false for a spec that doesn't fit.
pub fn within(mode: i64, spec: &str, at: &NaiveDateTime) -> bool {
    let Some(n) = numbers(spec) else {
        return false;
    };
    let (year, month, day) = (
        i64::from(at.year()),
        i64::from(at.month()),
        i64::from(at.day()),
    );
    let (hour, minute) = (i64::from(at.hour()), i64::from(at.minute()));
    // Each moment as one number that orders like time does.
    let key =
        |y: i64, mo: i64, d: i64, h: i64, mi: i64| (((y * 13 + mo) * 32 + d) * 24 + h) * 60 + mi;
    let in_range = |start: i64, now: i64, end: i64| start <= now && now <= end;
    match (mode, n.as_slice()) {
        (0, [y0, m0, d0, h0, i0, y1, m1, d1, h1, i1]) => in_range(
            key(*y0, *m0, *d0, *h0, *i0),
            key(year, month, day, hour, minute),
            key(*y1, *m1, *d1, *h1, *i1),
        ),
        (1, [m0, d0, h0, i0, m1, d1, h1, i1]) => in_range(
            key(0, *m0, *d0, *h0, *i0),
            key(0, month, day, hour, minute),
            key(0, *m1, *d1, *h1, *i1),
        ),
        (2, [d0, h0, i0, d1, h1, i1]) => in_range(
            key(0, 0, *d0, *h0, *i0),
            key(0, 0, day, hour, minute),
            key(0, 0, *d1, *h1, *i1),
        ),
        (3, [w0, h0, i0, w1, h1, i1]) => {
            let weekday = i64::from(at.weekday().num_days_from_sunday());
            let minutes = |w: i64, h: i64, m: i64| (w * 24 + h) * 60 + m;
            let (start, end) = (minutes(*w0, *h0, *i0), minutes(*w1, *h1, *i1));
            in_range(start, minutes(weekday, hour, minute), end)
                || weekday == 0 && in_range(start, minutes(7, hour, minute), end)
        }
        (4, [h0, i0, h1, i1]) => in_range(h0 * 60 + i0, hour * 60 + minute, h1 * 60 + i1),
        (5, [a, b]) => in_range(*a, minute, *b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_of_every_mode() {
        // Saturday 2026-10-17 15:30.
        let at = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|d| d.and_hms_opt(15, 30, 0))
            .unwrap();
        assert!(within(0, "2026-10-01 00:00 2026-10-31 23:59", &at));
        assert!(!within(0, "2025-10-01 00:00 2025-10-31 23:59", &at));
        assert!(within(1, "10-17 15:00 10-17 16:00", &at));
        assert!(within(2, "1 00:00 20 00:00", &at));
        // TQ's guild war: Saturday 14:00 to Sunday 21:00, Sunday as 7.
        assert!(within(3, "6 14:00 7 20:59", &at));
        let sunday = at + chrono::Duration::days(1);
        assert!(within(3, "6 14:00 7 20:59", &sunday));
        assert!(!within(
            3,
            "6 14:00 7 20:59",
            &(sunday + chrono::Duration::hours(6))
        ));
        assert!(within(3, "6 14:00 6 23:59", &at));
        assert!(!within(3, "0 14:00 0 23:59", &at));
        assert!(within(4, "15:30 15:30", &at));
        assert!(!within(5, "00 01", &at));
        assert!(!within(4, "nonsense", &at));
    }
}
