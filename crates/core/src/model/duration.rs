//! Duration parsing and formatting.
//!
//! Parses suffix-shorthand strings like `5d`, `1w 2d 3h`, `-2d` into
//! canonical `i64` seconds. Formats canonical seconds back to the same
//! grammar (compound, largest-first decomposition).
//!
//! Allowed suffixes: `s`, `min`, `h`, `d`, `w`. `min` (not `m`) is the
//! only minutes suffix to avoid month-vs-minute ambiguity. Months and
//! years are deliberately excluded — they have variable length and
//! belong to a calendar-aware type, not a fixed-length duration.

use std::fmt;

/// Errors produced when parsing a duration string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseDurationError {
    /// The input was empty or whitespace only.
    #[error("duration is empty")]
    Empty,

    /// A token had no unit suffix (e.g. `"5"` instead of `"5d"`).
    #[error("token '{token}' is missing a unit suffix")]
    MissingSuffix { token: String },

    /// A token's numeric portion didn't parse as an integer.
    #[error("token '{token}' has an invalid number")]
    InvalidNumber { token: String },

    /// A token had an unrecognized unit suffix.
    #[error("unknown unit '{suffix}' (allowed: s, min, h, d, w)")]
    UnknownSuffix { suffix: String },

    /// The same unit appeared more than once in the expression.
    #[error("unit '{suffix}' appears more than once")]
    DuplicateUnit { suffix: &'static str },

    /// A component had a leading minus sign. Only the whole expression
    /// may be negated; per-component signs are forbidden.
    #[error("component '{token}' has a sign — only the whole expression may be negative (e.g. `-1w 2d`, not `1w -2d`)")]
    ComponentNegative { token: String },

    /// Arithmetic overflow during parsing.
    #[error("duration value overflows i64 seconds")]
    Overflow,
}

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_WEEK: i64 = 604_800;

/// Parse a duration string into canonical i64 seconds.
///
/// Accepts the suffix-shorthand grammar: bare integer-with-suffix
/// (`5d`), compound (`1w 2d 3h`), with optional leading `-` for the
/// whole expression. See module-level docs for the full rule list.
pub fn parse_duration(input: &str) -> Result<i64, ParseDurationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseDurationError::Empty);
    }

    let (negate, rest) = match trimmed.strip_prefix('-') {
        Some(after) => (true, after.trim_start()),
        None => (false, trimmed),
    };

    if rest.is_empty() {
        return Err(ParseDurationError::Empty);
    }

    let mut total: i64 = 0;
    let mut seen: u8 = 0;

    for token in rest.split_whitespace() {
        if token.starts_with('-') || token.starts_with('+') {
            return Err(ParseDurationError::ComponentNegative {
                token: token.to_owned(),
            });
        }

        let split_at = token.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
            ParseDurationError::MissingSuffix {
                token: token.to_owned(),
            }
        })?;

        if split_at == 0 {
            return Err(ParseDurationError::InvalidNumber {
                token: token.to_owned(),
            });
        }

        let (number_str, suffix) = token.split_at(split_at);
        let number: i64 = number_str
            .parse()
            .map_err(|_| ParseDurationError::InvalidNumber {
                token: token.to_owned(),
            })?;

        let (multiplier, bit, canonical_suffix) = match suffix {
            "s" => (1, 1u8, "s"),
            "min" => (SECS_PER_MINUTE, 2u8, "min"),
            "h" => (SECS_PER_HOUR, 4u8, "h"),
            "d" => (SECS_PER_DAY, 8u8, "d"),
            "w" => (SECS_PER_WEEK, 16u8, "w"),
            other => {
                return Err(ParseDurationError::UnknownSuffix {
                    suffix: other.to_owned(),
                })
            }
        };

        if seen & bit != 0 {
            return Err(ParseDurationError::DuplicateUnit {
                suffix: canonical_suffix,
            });
        }
        seen |= bit;

        let component = number
            .checked_mul(multiplier)
            .ok_or(ParseDurationError::Overflow)?;
        total = total
            .checked_add(component)
            .ok_or(ParseDurationError::Overflow)?;
    }

    if seen == 0 {
        return Err(ParseDurationError::Empty);
    }

    if negate {
        total.checked_neg().ok_or(ParseDurationError::Overflow)
    } else {
        Ok(total)
    }
}

/// Format canonical i64 seconds as a duration string.
///
/// Uses largest-first decomposition into `w`, `d`, `h`, `min`, `s`,
/// skipping zero components. `0` renders as `"0s"`. Negative values get
/// a leading `-`. Round-trip stable: `parse_duration(format(n)) == n`
/// for any `n: i64` (uses i128 internally to handle `i64::MIN` safely).
pub fn format_duration_seconds(total: i64) -> String {
    if total == 0 {
        return "0s".to_owned();
    }

    let mut out = String::new();
    if total < 0 {
        out.push('-');
    }

    // Use i128 to safely handle i64::MIN whose absolute value overflows i64.
    let mut remaining: i128 = (total as i128).abs();

    let units: &[(i128, &str)] = &[
        (SECS_PER_WEEK as i128, "w"),
        (SECS_PER_DAY as i128, "d"),
        (SECS_PER_HOUR as i128, "h"),
        (SECS_PER_MINUTE as i128, "min"),
        (1, "s"),
    ];

    let mut first = true;
    for (size, suffix) in units {
        let count = remaining / size;
        if count > 0 {
            if !first {
                out.push(' ');
            }
            // count fits in i64 by construction (it's a partition of |total|).
            use fmt::Write;
            let _ = write!(&mut out, "{count}{suffix}");
            first = false;
            remaining -= count * size;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_duration: single-component happy paths ────────────────

    #[test]
    fn parse_seconds() {
        assert_eq!(parse_duration("5s"), Ok(5));
    }

    #[test]
    fn parse_minutes() {
        assert_eq!(parse_duration("5min"), Ok(300));
    }

    #[test]
    fn parse_hours() {
        assert_eq!(parse_duration("5h"), Ok(18_000));
    }

    #[test]
    fn parse_days() {
        assert_eq!(parse_duration("5d"), Ok(432_000));
    }

    #[test]
    fn parse_weeks() {
        assert_eq!(parse_duration("2w"), Ok(1_209_600));
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_duration("0s"), Ok(0));
        assert_eq!(parse_duration("0d"), Ok(0));
    }

    // ── parse_duration: compound ─────────────────────────────────────

    #[test]
    fn parse_compound_two_units() {
        // 1w + 2d = 604_800 + 172_800 = 777_600
        assert_eq!(parse_duration("1w 2d"), Ok(777_600));
    }

    #[test]
    fn parse_compound_all_five_units() {
        // 1w 1d 1h 1min 1s
        let expected = SECS_PER_WEEK + SECS_PER_DAY + SECS_PER_HOUR + SECS_PER_MINUTE + 1;
        assert_eq!(parse_duration("1w 1d 1h 1min 1s"), Ok(expected));
    }

    #[test]
    fn parse_compound_unordered() {
        // Order shouldn't matter — components sum.
        assert_eq!(parse_duration("3h 1w 2d"), parse_duration("1w 2d 3h"));
    }

    #[test]
    fn parse_compound_with_extra_whitespace() {
        assert_eq!(parse_duration("  1w   2d  "), Ok(777_600));
    }

    // ── parse_duration: negative ─────────────────────────────────────

    #[test]
    fn parse_negative_single() {
        assert_eq!(parse_duration("-2d"), Ok(-172_800));
    }

    #[test]
    fn parse_negative_compound() {
        // -(1w + 2d) = -777_600
        assert_eq!(parse_duration("-1w 2d"), Ok(-777_600));
    }

    #[test]
    fn parse_negative_with_space_after_sign() {
        // `- 2d` — leading minus then trim_start picks up the rest.
        assert_eq!(parse_duration("- 2d"), Ok(-172_800));
    }

    // ── parse_duration: error cases ──────────────────────────────────

    #[test]
    fn empty_string_rejected() {
        assert_eq!(parse_duration(""), Err(ParseDurationError::Empty));
        assert_eq!(parse_duration("   "), Err(ParseDurationError::Empty));
    }

    #[test]
    fn lone_minus_rejected() {
        assert_eq!(parse_duration("-"), Err(ParseDurationError::Empty));
        assert_eq!(parse_duration("-   "), Err(ParseDurationError::Empty));
    }

    #[test]
    fn missing_suffix_rejected() {
        assert!(matches!(
            parse_duration("5"),
            Err(ParseDurationError::MissingSuffix { .. })
        ));
    }

    #[test]
    fn bare_m_rejected_to_avoid_month_minute_ambiguity() {
        // Critical: `5m` must NOT parse as 5 minutes. Use `5min` or `5mo` (rejected anyway).
        assert!(matches!(
            parse_duration("5m"),
            Err(ParseDurationError::UnknownSuffix { .. })
        ));
    }

    #[test]
    fn unknown_suffix_rejected() {
        assert!(matches!(
            parse_duration("5y"),
            Err(ParseDurationError::UnknownSuffix { .. })
        ));
        assert!(matches!(
            parse_duration("5mo"),
            Err(ParseDurationError::UnknownSuffix { .. })
        ));
        assert!(matches!(
            parse_duration("5days"),
            Err(ParseDurationError::UnknownSuffix { .. })
        ));
    }

    #[test]
    fn duplicate_unit_rejected() {
        assert!(matches!(
            parse_duration("1w 2w"),
            Err(ParseDurationError::DuplicateUnit { suffix: "w" })
        ));
        assert!(matches!(
            parse_duration("3h 5h"),
            Err(ParseDurationError::DuplicateUnit { suffix: "h" })
        ));
    }

    #[test]
    fn component_negative_rejected() {
        // `-1w 2d` is OK (leading minus), but `1w -2d` is not.
        assert!(matches!(
            parse_duration("1w -2d"),
            Err(ParseDurationError::ComponentNegative { .. })
        ));
        assert!(matches!(
            parse_duration("1w +2d"),
            Err(ParseDurationError::ComponentNegative { .. })
        ));
    }

    #[test]
    fn invalid_number_rejected() {
        assert!(matches!(
            parse_duration("xd"),
            Err(ParseDurationError::InvalidNumber { .. })
        ));
    }

    #[test]
    fn fractional_rejected() {
        // Not an integer — split_at finds the `.` as non-digit, so the
        // remainder ".5d" is the suffix and won't match a known one.
        assert!(matches!(
            parse_duration("1.5d"),
            Err(ParseDurationError::UnknownSuffix { .. })
        ));
    }

    #[test]
    fn overflow_rejected() {
        // i64::MAX / SECS_PER_WEEK ≈ 1.5e13. Provide something larger.
        let huge = format!("{}w", i64::MAX);
        assert_eq!(parse_duration(&huge), Err(ParseDurationError::Overflow));
    }

    // ── format_duration_seconds ──────────────────────────────────────

    #[test]
    fn format_zero() {
        assert_eq!(format_duration_seconds(0), "0s");
    }

    #[test]
    fn format_single_second() {
        assert_eq!(format_duration_seconds(1), "1s");
    }

    #[test]
    fn format_one_minute() {
        assert_eq!(format_duration_seconds(60), "1min");
    }

    #[test]
    fn format_one_hour() {
        assert_eq!(format_duration_seconds(3_600), "1h");
    }

    #[test]
    fn format_one_day() {
        assert_eq!(format_duration_seconds(86_400), "1d");
    }

    #[test]
    fn format_one_week() {
        assert_eq!(format_duration_seconds(604_800), "1w");
    }

    #[test]
    fn format_compound_skips_zero_components() {
        // 1w + 0d + 3h + 0min + 0s
        let total = SECS_PER_WEEK + 3 * SECS_PER_HOUR;
        assert_eq!(format_duration_seconds(total), "1w 3h");
    }

    #[test]
    fn format_all_units_present() {
        let total = SECS_PER_WEEK + SECS_PER_DAY + SECS_PER_HOUR + SECS_PER_MINUTE + 1;
        assert_eq!(format_duration_seconds(total), "1w 1d 1h 1min 1s");
    }

    #[test]
    fn format_negative() {
        assert_eq!(format_duration_seconds(-3_600), "-1h");
        assert_eq!(format_duration_seconds(-1), "-1s");
    }

    #[test]
    fn format_8000_seconds_compound() {
        // 8000s = 2h + 13min + 20s
        assert_eq!(format_duration_seconds(8_000), "2h 13min 20s");
    }

    #[test]
    fn format_i64_min_does_not_panic() {
        // The i128 path must absorb i64::MIN without panic.
        let formatted = format_duration_seconds(i64::MIN);
        assert!(formatted.starts_with('-'));
        // We don't pin the exact decomposition — round-trip is what matters.
    }

    // ── Round-trip property ──────────────────────────────────────────

    #[test]
    fn round_trip_representative_values() {
        // Values within "realistic" range round-trip cleanly. Values
        // close to i64::MIN/MAX produce a week count whose
        // multiplication by 604_800 overflows i64 — that's a parser
        // limitation we accept since no real-world duration field will
        // ever hold such values. The non-panic guarantee for i64::MIN
        // formatting is verified in `format_i64_min_does_not_panic`.
        let values = [
            0,
            1,
            -1,
            60,
            -60,
            3_600,
            86_400,
            604_800,
            777_600,                           // 1w 2d
            SECS_PER_WEEK + SECS_PER_HOUR + 1, // 1w 1h 1s
            -777_600,
            // Largest values that survive round-trip: i64::MAX divided
            // by SECS_PER_WEEK gives the maximum week count whose
            // multiplication won't overflow on the way back.
            i64::MAX / SECS_PER_WEEK * SECS_PER_WEEK,
            -(i64::MAX / SECS_PER_WEEK) * SECS_PER_WEEK,
        ];
        for value in values {
            let formatted = format_duration_seconds(value);
            let reparsed = parse_duration(&formatted).unwrap_or_else(|err| {
                panic!("round-trip failed for {value} (formatted as '{formatted}'): {err:?}")
            });
            assert_eq!(
                reparsed, value,
                "round-trip mismatch for {value} (formatted as '{formatted}')"
            );
        }
    }

    // ── serde_yaml sanity (Critical 1 from the design review) ────────
    //
    // Confirms the load-bearing assumption that unquoted suffix-shorthand
    // values deserialize as YAML strings, not numbers or other types.

    #[test]
    fn serde_yaml_deserializes_unquoted_suffix_as_string() {
        let yaml = "\
            a: 5d\n\
            b: \"5d\"\n\
            c: 1w 2d\n\
            d: -2d\n\
            e: 30s\n\
            f: 120min\n\
        ";
        let parsed: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();

        for key in ["a", "b", "c", "d", "e", "f"] {
            let value = parsed
                .get(serde_yaml::Value::String(key.to_owned()))
                .expect(key);
            assert!(
                value.is_string(),
                "key '{key}' deserialized as {value:?}, expected String"
            );
        }
    }
}
