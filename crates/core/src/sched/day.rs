//! "Day" = local calendar date after shifting by the rollover hour (SPEC §7.4).

use chrono::{DateTime, Days, NaiveDate, TimeDelta, Utc};

use crate::domain::CardSched;

pub fn local_day(ts_utc: DateTime<Utc>, tz_offset_min: i32, rollover_hour: u32) -> NaiveDate {
    (ts_utc + TimeDelta::minutes(i64::from(tz_offset_min))
        - TimeDelta::hours(i64::from(rollover_hour)))
    .date_naive()
}

/// Whole days from `from` to `to`, never negative.
pub fn elapsed_days(from: NaiveDate, to: NaiveDate) -> u32 {
    u32::try_from((to - from).num_days()).unwrap_or(0)
}

/// The local day of the review that produced this state: `due_day - interval_days`.
pub fn review_day(sched: &CardSched) -> NaiveDate {
    sched.due_day - Days::new(u64::from(sched.interval_days))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn rollover_hour_shifts_the_date() {
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        // 03:30 local on the 2nd still belongs to the 1st with a 04:00 rollover.
        assert_eq!(
            local_day(utc("2026-03-02T03:30:00Z"), 0, 4),
            d("2026-03-01")
        );
        assert_eq!(
            local_day(utc("2026-03-02T04:00:00Z"), 0, 4),
            d("2026-03-02")
        );
        // UTC-6: 09:00Z is 03:00 local → previous day.
        assert_eq!(
            local_day(utc("2026-03-02T09:00:00Z"), -360, 4),
            d("2026-03-01")
        );
        assert_eq!(elapsed_days(d("2026-03-01"), d("2026-03-04")), 3);
        assert_eq!(elapsed_days(d("2026-03-04"), d("2026-03-01")), 0);
    }
}
