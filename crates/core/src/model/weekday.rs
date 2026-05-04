//! Day-of-week enum used by the project working calendar.
//!
//! `chrono::Weekday` exists but serializes as `Mon`/`Tue`; we want the
//! full lowercase day name in YAML so consumers don't memorize an
//! abbreviation table. This enum bridges between the YAML form and
//! `chrono::Weekday` for date math.

use serde::{Deserialize, Serialize};

/// One day of the week. Serialized as the full lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// Convert from `chrono::Weekday`.
    pub fn from_chrono(weekday: chrono::Weekday) -> Self {
        match weekday {
            chrono::Weekday::Mon => Weekday::Monday,
            chrono::Weekday::Tue => Weekday::Tuesday,
            chrono::Weekday::Wed => Weekday::Wednesday,
            chrono::Weekday::Thu => Weekday::Thursday,
            chrono::Weekday::Fri => Weekday::Friday,
            chrono::Weekday::Sat => Weekday::Saturday,
            chrono::Weekday::Sun => Weekday::Sunday,
        }
    }

    /// Convert to `chrono::Weekday`.
    pub fn to_chrono(self) -> chrono::Weekday {
        match self {
            Weekday::Monday => chrono::Weekday::Mon,
            Weekday::Tuesday => chrono::Weekday::Tue,
            Weekday::Wednesday => chrono::Weekday::Wed,
            Weekday::Thursday => chrono::Weekday::Thu,
            Weekday::Friday => chrono::Weekday::Fri,
            Weekday::Saturday => chrono::Weekday::Sat,
            Weekday::Sunday => chrono::Weekday::Sun,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_full_lowercase_name() {
        assert_eq!(
            serde_yaml::to_string(&Weekday::Monday).unwrap().trim(),
            "monday"
        );
        assert_eq!(
            serde_yaml::to_string(&Weekday::Sunday).unwrap().trim(),
            "sunday"
        );
    }

    #[test]
    fn deserializes_full_lowercase_name() {
        let parsed: Weekday = serde_yaml::from_str("wednesday").unwrap();
        assert_eq!(parsed, Weekday::Wednesday);
    }

    #[test]
    fn rejects_abbreviation() {
        assert!(serde_yaml::from_str::<Weekday>("mon").is_err());
        assert!(serde_yaml::from_str::<Weekday>("tue").is_err());
    }

    #[test]
    fn rejects_uppercase_or_titlecase() {
        assert!(serde_yaml::from_str::<Weekday>("Monday").is_err());
        assert!(serde_yaml::from_str::<Weekday>("MONDAY").is_err());
    }

    #[test]
    fn rejects_unknown_day() {
        assert!(serde_yaml::from_str::<Weekday>("funday").is_err());
    }

    #[test]
    fn chrono_round_trip() {
        for original in [
            chrono::Weekday::Mon,
            chrono::Weekday::Tue,
            chrono::Weekday::Wed,
            chrono::Weekday::Thu,
            chrono::Weekday::Fri,
            chrono::Weekday::Sat,
            chrono::Weekday::Sun,
        ] {
            let round_tripped = Weekday::from_chrono(original).to_chrono();
            assert_eq!(original, round_tripped);
        }
    }
}
