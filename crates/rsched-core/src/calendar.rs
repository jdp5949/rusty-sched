//! Calendars: control when jobs are allowed to run.

use crate::{CalendarId, CoreError};
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// One rule in a calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalendarRule {
    /// Allow on these weekdays (Mon=1..Sun=7).
    Weekdays {
        /// Days (1..=7).
        days: Vec<u8>,
    },
    /// Block on these dates (YYYY-MM-DD).
    Blackout {
        /// Dates blocked.
        dates: Vec<NaiveDate>,
    },
    /// Allow only inside this time-of-day window (UTC).
    TimeWindow {
        /// Start (HH:MM:SS).
        start: NaiveTime,
        /// End exclusive (HH:MM:SS).
        end: NaiveTime,
    },
}

/// Calendar = id + name + ordered list of rules. Rules are ANDed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    /// Identifier.
    pub id: CalendarId,
    /// Display name (unique).
    pub name: String,
    /// Ordered AND-combined rules.
    pub rules: Vec<CalendarRule>,
}

impl Calendar {
    /// True if `now` satisfies every rule.
    pub fn allows(&self, now: DateTime<Utc>) -> bool {
        self.rules.iter().all(|r| rule_allows(r, now))
    }

    /// Validate calendar shape.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::InvalidName(self.name.clone(), "name empty"));
        }
        for r in &self.rules {
            match r {
                CalendarRule::Weekdays { days } => {
                    if days.is_empty() || days.iter().any(|d| *d < 1 || *d > 7) {
                        return Err(CoreError::InvalidCalendar("weekdays out of range"));
                    }
                }
                CalendarRule::TimeWindow { start, end } => {
                    if start >= end {
                        return Err(CoreError::InvalidCalendar("time window start>=end"));
                    }
                }
                CalendarRule::Blackout { .. } => {}
            }
        }
        Ok(())
    }
}

fn rule_allows(rule: &CalendarRule, now: DateTime<Utc>) -> bool {
    match rule {
        CalendarRule::Weekdays { days } => {
            let dow = match now.weekday() {
                Weekday::Mon => 1,
                Weekday::Tue => 2,
                Weekday::Wed => 3,
                Weekday::Thu => 4,
                Weekday::Fri => 5,
                Weekday::Sat => 6,
                Weekday::Sun => 7,
            };
            days.contains(&dow)
        }
        CalendarRule::Blackout { dates } => !dates.contains(&now.date_naive()),
        CalendarRule::TimeWindow { start, end } => {
            let t = NaiveTime::from_hms_opt(now.hour(), now.minute(), now.second()).unwrap();
            &t >= start && &t < end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn weekday_allows() {
        let c = Calendar {
            id: CalendarId::new(),
            name: "weekdays".into(),
            rules: vec![CalendarRule::Weekdays {
                days: vec![1, 2, 3, 4, 5],
            }],
        };
        // 2026-05-18 is a Monday
        assert!(c.allows(ts(2026, 5, 18, 12, 0)));
        // 2026-05-23 is a Saturday
        assert!(!c.allows(ts(2026, 5, 23, 12, 0)));
    }

    #[test]
    fn blackout_blocks() {
        let c = Calendar {
            id: CalendarId::new(),
            name: "no-may-25".into(),
            rules: vec![CalendarRule::Blackout {
                dates: vec![NaiveDate::from_ymd_opt(2026, 5, 25).unwrap()],
            }],
        };
        assert!(!c.allows(ts(2026, 5, 25, 10, 0)));
        assert!(c.allows(ts(2026, 5, 26, 10, 0)));
    }

    #[test]
    fn time_window_works() {
        let c = Calendar {
            id: CalendarId::new(),
            name: "biz-hours".into(),
            rules: vec![CalendarRule::TimeWindow {
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            }],
        };
        assert!(!c.allows(ts(2026, 5, 18, 8, 59)));
        assert!(c.allows(ts(2026, 5, 18, 9, 0)));
        assert!(c.allows(ts(2026, 5, 18, 16, 59)));
        assert!(!c.allows(ts(2026, 5, 18, 17, 0)));
    }

    #[test]
    fn rules_anded() {
        let c = Calendar {
            id: CalendarId::new(),
            name: "weekday-biz".into(),
            rules: vec![
                CalendarRule::Weekdays {
                    days: vec![1, 2, 3, 4, 5],
                },
                CalendarRule::TimeWindow {
                    start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
                },
            ],
        };
        assert!(c.allows(ts(2026, 5, 18, 10, 0))); // Mon 10am ok
        assert!(!c.allows(ts(2026, 5, 18, 20, 0))); // Mon 8pm bad
        assert!(!c.allows(ts(2026, 5, 23, 10, 0))); // Sat 10am bad
    }

    #[test]
    fn invalid_weekday() {
        let c = Calendar {
            id: CalendarId::new(),
            name: "x".into(),
            rules: vec![CalendarRule::Weekdays { days: vec![0, 9] }],
        };
        assert!(c.validate().is_err());
    }
}
