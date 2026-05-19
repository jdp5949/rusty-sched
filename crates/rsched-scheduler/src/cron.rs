//! Cron expression evaluation. Wraps `croner` so the rest of the crate is
//! independent of the parser choice.

use crate::SchedulerError;
use chrono::{DateTime, Utc};
use croner::Cron;

/// Compute the next fire time strictly greater than `after`, evaluated in the
/// given IANA timezone (defaults to UTC if `tz` is `None`).
pub fn next_fire(
    expr: &str,
    tz: Option<&str>,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, SchedulerError> {
    let cron = Cron::new(expr)
        .parse()
        .map_err(|e| SchedulerError::Cron(format!("{e}")))?;
    match tz {
        None => {
            let next = cron
                .find_next_occurrence(&after, false)
                .map_err(|e| SchedulerError::Cron(format!("{e}")))?;
            Ok(next)
        }
        Some(tz) => {
            let tz: chrono_tz::Tz = tz
                .parse()
                .map_err(|_| SchedulerError::Cron(format!("unknown tz {tz}")))?;
            let after_local = after.with_timezone(&tz);
            let next = cron
                .find_next_occurrence(&after_local, false)
                .map_err(|e| SchedulerError::Cron(format!("{e}")))?;
            Ok(next.with_timezone(&Utc))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    #[test]
    fn every_5_minutes() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();
        let next = next_fire("*/5 * * * *", None, start).unwrap();
        assert_eq!(next.minute() % 5, 0);
        assert!(next > start);
    }

    #[test]
    fn daily_at_2am() {
        let start = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
        let next = next_fire("0 2 * * *", None, start).unwrap();
        assert_eq!(next.hour(), 2);
        assert_eq!(next.day(), 19);
    }

    #[test]
    fn timezone_respected() {
        // 09:00 America/New_York = 13:00 UTC (winter) or 14:00 UTC (DST).
        let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let next = next_fire("0 9 * * *", Some("America/New_York"), start).unwrap();
        // 13:00 or 14:00 UTC depending on DST
        let h = next.hour();
        assert!(h == 13 || h == 14, "got {h}");
    }

    #[test]
    fn invalid_cron() {
        let start = Utc::now();
        assert!(next_fire("bogus", None, start).is_err());
    }

    #[test]
    fn unknown_tz() {
        let start = Utc::now();
        assert!(next_fire("* * * * *", Some("Mars/Olympus"), start).is_err());
    }
}
