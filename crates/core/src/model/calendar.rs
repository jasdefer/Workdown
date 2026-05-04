//! Working calendar — the set of weekdays that count as work days.
//!
//! Used by the workload extractor to decide which days an item's effort
//! distributes across. A `WorkingCalendar` is a stateless predicate over
//! `NaiveDate`; date-range walks (counting working days, etc.) live in
//! the consuming module to keep this type small.
//!
//! Defaults to Monday–Friday when the project doesn't set one in
//! `config.yaml`. Per-view overrides on `Workload` views supersede the
//! project-level default.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};

use super::weekday::Weekday;

/// Set of weekdays that count as work days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingCalendar {
    days: BTreeSet<Weekday>,
}

impl WorkingCalendar {
    /// Build a calendar from any iterable of weekdays. Duplicates are
    /// folded; order is irrelevant.
    pub fn from_days<I: IntoIterator<Item = Weekday>>(days: I) -> Self {
        Self {
            days: days.into_iter().collect(),
        }
    }

    /// The default workplace calendar: Monday through Friday.
    pub fn default_business_week() -> Self {
        Self::from_days([
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
        ])
    }

    /// True if the given date falls on a configured working day.
    pub fn is_working(&self, date: NaiveDate) -> bool {
        self.days.contains(&Weekday::from_chrono(date.weekday()))
    }

    /// True if the calendar has zero working days configured. Useful for
    /// callers that want to early-exit rather than do range math against
    /// an empty calendar.
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn default_business_week_includes_monday_to_friday() {
        let calendar = WorkingCalendar::default_business_week();
        // 2026-01-05 is a Monday, 2026-01-09 is a Friday.
        for offset in 0..5 {
            let day = ymd(2026, 1, 5) + chrono::Duration::days(offset);
            assert!(calendar.is_working(day), "{day} should be working");
        }
    }

    #[test]
    fn default_business_week_excludes_weekend() {
        let calendar = WorkingCalendar::default_business_week();
        // 2026-01-10 Sat, 2026-01-11 Sun.
        assert!(!calendar.is_working(ymd(2026, 1, 10)));
        assert!(!calendar.is_working(ymd(2026, 1, 11)));
    }

    #[test]
    fn custom_calendar_only_marks_listed_days() {
        let calendar = WorkingCalendar::from_days([Weekday::Tuesday, Weekday::Thursday]);
        // Mon Jan 5 → Sun Jan 11.
        let working: Vec<_> = (0..7)
            .map(|offset| {
                let day = ymd(2026, 1, 5) + chrono::Duration::days(offset);
                (day, calendar.is_working(day))
            })
            .collect();
        let expected: Vec<bool> = vec![false, true, false, true, false, false, false];
        let actual: Vec<bool> = working.iter().map(|(_, w)| *w).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn duplicate_days_are_folded() {
        let calendar =
            WorkingCalendar::from_days([Weekday::Monday, Weekday::Monday, Weekday::Monday]);
        assert!(calendar.is_working(ymd(2026, 1, 5)));
        // Tue Jan 6 — not configured.
        assert!(!calendar.is_working(ymd(2026, 1, 6)));
    }

    #[test]
    fn empty_calendar_marks_nothing_as_working() {
        let calendar = WorkingCalendar::from_days::<[Weekday; 0]>([]);
        assert!(calendar.is_empty());
        assert!(!calendar.is_working(ymd(2026, 1, 5)));
    }
}
