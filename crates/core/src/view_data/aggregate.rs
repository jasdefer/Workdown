//! Aggregate functions over typed field values.
//!
//! Called from bar_chart, metric, and heatmap extractors. Returns an
//! [`AggregateValue`] so the same helper serves numeric aggregates
//! (sum/avg/min/max over integer/float) and date aggregates (avg/min/max
//! on dates). `count` always returns `Number(n as f64)`.
//!
//! Inputs are filtered to the aggregate's expected type before reduction:
//! non-matching values are skipped silently (the extractor has already
//! routed `NonNumericValue` items to `unplaced`). An empty valid set
//! returns `None` for avg/min/max (drop the result) and `Number(0.0)`
//! for sum.

use chrono::{Datelike, NaiveDate};

use crate::model::views::Aggregate;
use crate::model::FieldValue;

use super::common::AggregateValue;

pub(super) fn compute_aggregate(
    values: &[&FieldValue],
    aggregate: Aggregate,
) -> Option<AggregateValue> {
    match aggregate {
        Aggregate::Count => Some(AggregateValue::Number(values.len() as f64)),
        Aggregate::Sum => Some(AggregateValue::Number(
            values.iter().copied().filter_map(as_number).sum(),
        )),
        Aggregate::Avg => average(values),
        Aggregate::Min => extremum(values, true),
        Aggregate::Max => extremum(values, false),
    }
}

fn as_number(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Integer(integer) => Some(*integer as f64),
        FieldValue::Float(float) => Some(*float),
        // Duration aggregates as canonical seconds. Chart axes show raw
        // seconds in v1 (no per-axis unit-formatting hook yet).
        FieldValue::Duration(seconds) => Some(*seconds as f64),
        _ => None,
    }
}

fn as_date(value: &FieldValue) -> Option<NaiveDate> {
    match value {
        FieldValue::Date(date) => Some(*date),
        _ => None,
    }
}

fn average(values: &[&FieldValue]) -> Option<AggregateValue> {
    let numbers: Vec<f64> = values.iter().copied().filter_map(as_number).collect();
    if !numbers.is_empty() {
        let sum: f64 = numbers.iter().sum();
        return Some(AggregateValue::Number(sum / numbers.len() as f64));
    }
    let dates: Vec<NaiveDate> = values.iter().copied().filter_map(as_date).collect();
    if !dates.is_empty() {
        // Day-count mean — midpoint semantics. CE day numbering is
        // arbitrary but stable, and the result is a NaiveDate regardless.
        let sum_days: i64 = dates
            .iter()
            .map(|date| date.num_days_from_ce() as i64)
            .sum();
        let avg_days = sum_days / dates.len() as i64;
        return NaiveDate::from_num_days_from_ce_opt(avg_days as i32).map(AggregateValue::Date);
    }
    None
}

fn extremum(values: &[&FieldValue], pick_min: bool) -> Option<AggregateValue> {
    let numbers: Vec<f64> = values.iter().copied().filter_map(as_number).collect();
    if !numbers.is_empty() {
        let result = if pick_min {
            numbers.iter().copied().fold(f64::INFINITY, f64::min)
        } else {
            numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        };
        return Some(AggregateValue::Number(result));
    }
    let dates: Vec<NaiveDate> = values.iter().copied().filter_map(as_date).collect();
    if !dates.is_empty() {
        let result = if pick_min {
            *dates.iter().min().unwrap()
        } else {
            *dates.iter().max().unwrap()
        };
        return Some(AggregateValue::Date(result));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn number(n: f64) -> FieldValue {
        FieldValue::Float(n)
    }

    fn date(y: i32, m: u32, d: u32) -> FieldValue {
        FieldValue::Date(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    #[test]
    fn count_includes_all_values_regardless_of_type() {
        let values = [
            FieldValue::String("x".into()),
            number(1.0),
            date(2026, 1, 1),
        ];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let result = compute_aggregate(&refs, Aggregate::Count);
        assert!(matches!(result, Some(AggregateValue::Number(n)) if n == 3.0));
    }

    #[test]
    fn sum_over_numbers() {
        let values = [number(1.0), number(2.5), number(0.5)];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let result = compute_aggregate(&refs, Aggregate::Sum);
        assert!(matches!(result, Some(AggregateValue::Number(n)) if (n - 4.0).abs() < 1e-9));
    }

    #[test]
    fn sum_empty_returns_zero() {
        let refs: Vec<&FieldValue> = Vec::new();
        let result = compute_aggregate(&refs, Aggregate::Sum);
        assert!(matches!(result, Some(AggregateValue::Number(n)) if n == 0.0));
    }

    #[test]
    fn avg_over_numbers() {
        let values = [number(2.0), number(4.0), number(6.0)];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let result = compute_aggregate(&refs, Aggregate::Avg);
        assert!(matches!(result, Some(AggregateValue::Number(n)) if (n - 4.0).abs() < 1e-9));
    }

    #[test]
    fn avg_over_dates_is_midpoint() {
        let values = [date(2026, 1, 1), date(2026, 1, 5)];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let result = compute_aggregate(&refs, Aggregate::Avg);
        assert_eq!(
            result,
            Some(AggregateValue::Date(
                NaiveDate::from_ymd_opt(2026, 1, 3).unwrap()
            ))
        );
    }

    #[test]
    fn min_max_over_numbers() {
        let values = [number(3.0), number(1.0), number(2.0)];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let min = compute_aggregate(&refs, Aggregate::Min);
        let max = compute_aggregate(&refs, Aggregate::Max);
        assert!(matches!(min, Some(AggregateValue::Number(n)) if n == 1.0));
        assert!(matches!(max, Some(AggregateValue::Number(n)) if n == 3.0));
    }

    #[test]
    fn min_max_over_dates() {
        let values = [date(2026, 5, 1), date(2026, 1, 1), date(2026, 3, 1)];
        let refs: Vec<&FieldValue> = values.iter().collect();
        let min = compute_aggregate(&refs, Aggregate::Min);
        let max = compute_aggregate(&refs, Aggregate::Max);
        assert_eq!(
            min,
            Some(AggregateValue::Date(
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
            ))
        );
        assert_eq!(
            max,
            Some(AggregateValue::Date(
                NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()
            ))
        );
    }

    #[test]
    fn avg_empty_returns_none() {
        let refs: Vec<&FieldValue> = Vec::new();
        assert_eq!(compute_aggregate(&refs, Aggregate::Avg), None);
        assert_eq!(compute_aggregate(&refs, Aggregate::Min), None);
        assert_eq!(compute_aggregate(&refs, Aggregate::Max), None);
    }
}
