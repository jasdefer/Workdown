//! `--delta` on `duration` and `date` fields.
//!
//! Both share the same operand shape (signed seconds, parsed from a
//! duration literal like `1w 2d`). They differ in the storage format
//! (duration string vs `YYYY-MM-DD`), the arithmetic, and what they make
//! of an absent field: a duration counts as `0s` and is created by the
//! delta, a date has no zero to count from and asks for a value first.

use std::collections::HashMap;

use super::{current_value, ComputedMutation, SetError};

/// Reject `--delta` on a duration field only when the current value is
/// something other than a duration.
///
/// An absent duration is allowed through — it counts as `0s` and the
/// delta creates the field, because the first time anyone records effort
/// on an item is exactly the moment the field is absent. A value that is
/// present but isn't a duration still fails: replacing a typo with a
/// measured number would destroy the evidence that something was wrong.
pub(super) fn require_absent_or_valid_duration(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
        None => Ok(()),
        Some(serde_yaml::Value::String(string))
            if crate::model::duration::parse_duration(string).is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "duration string (e.g. '1w 2d', '-3h')",
        }),
    }
}

/// Reject `--delta` when the date field is absent or its current value
/// isn't a parseable `YYYY-MM-DD` date.
///
/// Unlike a duration, a date has no zero to count from, so an absent
/// date stays an error — `--delta` works on dates, it just needs a value
/// to move.
pub(super) fn require_existing_date(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string))
            if chrono::NaiveDate::parse_from_str(string, "%Y-%m-%d").is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "date (YYYY-MM-DD)",
        }),
    }
}

pub(super) fn compute_duration_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    // An absent field starts at zero: an absent duration and `0s` reach
    // the same answer. A derived value — rolled up, computed or pulled —
    // never appears in the frontmatter map this reads, so it starts at
    // zero too, rather than freezing the derived number into the file
    // where it would go stale the next time a child changed.
    let current_seconds = match previous_value.as_ref() {
        None => 0,
        Some(value) => {
            let current_string = value
                .as_str()
                .expect("precondition ensures a duration string when a value is present");
            crate::model::duration::parse_duration(current_string)
                .expect("precondition ensures parseable duration")
        }
    };
    let new_seconds = current_seconds.saturating_add(delta_seconds);
    let new_string = crate::model::duration::format_duration_seconds(new_seconds);
    let new_value = serde_yaml::Value::String(new_string);
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

pub(super) fn compute_date_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    let current_string = previous_value
        .as_ref()
        .and_then(|value| value.as_str())
        .expect("precondition ensures existing date string");
    let current_date = chrono::NaiveDate::parse_from_str(current_string, "%Y-%m-%d")
        .expect("precondition ensures parseable date");
    let new_date = current_date
        .checked_add_signed(chrono::Duration::seconds(delta_seconds))
        .expect("date arithmetic must fit chrono's NaiveDate range");
    let new_value = serde_yaml::Value::String(new_date.format("%Y-%m-%d").to_string());
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use crate::model::WorkItemId;
    use crate::operations::set::*;

    // ── Delta: duration ──────────────────────────────────────────────

    #[test]
    fn delta_on_duration_adds_seconds() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 2d\n---\n",
        );

        // +1d = 86_400 seconds
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(86_400)),
        )
        .unwrap();

        let new_string = outcome.new_value.unwrap();
        assert_eq!(new_string.as_str().unwrap(), "3d");
    }

    #[test]
    fn delta_on_duration_with_negative_subtracts() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 1w\n---\n",
        );

        // -3d = -259_200 seconds. 1w - 3d = 4d.
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "4d");
    }

    // ── Delta: duration on an absent field ───────────────────────────

    /// Run `--delta` on `estimate` against a one-item project whose
    /// frontmatter is `content`, and return the outcome.
    fn delta_estimate(content: &str, delta_seconds: i64) -> SetOutcome {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", content);

        run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(delta_seconds)),
        )
        .unwrap()
    }

    #[test]
    fn delta_on_absent_duration_field_creates_it_from_zero() {
        // +30min = 1_800 seconds
        let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", 1_800);

        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    }

    #[test]
    fn delta_on_duration_field_written_with_no_value_creates_it_from_zero() {
        // `estimate:` parses as YAML null — nothing to start from, and
        // nothing a delta could destroy either.
        let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\nestimate:\n---\n", 1_800);

        assert!(
            outcome.previous_value.is_none(),
            "a null on disk reports as absent, not as `null` — the file never said `null`"
        );
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    }

    #[test]
    fn delta_on_duration_field_holding_an_empty_string_creates_it_from_zero() {
        let outcome = delta_estimate(
            "---\ntitle: Task 1\nstatus: open\nestimate: ''\n---\n",
            1_800,
        );

        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    }

    #[test]
    fn negative_delta_on_absent_duration_field_writes_a_negative_duration() {
        // Zero minus thirty minutes is minus thirty minutes. A project
        // that considers that nonsense sets `min: 0` on the field.
        let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", -1_800);

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "-30min");
    }

    #[test]
    fn zero_delta_on_absent_duration_field_creates_it_at_zero() {
        // A delta always writes. Deciding a short session records nothing
        // belongs to whatever measured it, before it asks for a delta.
        let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", 0);

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "0s");
    }

    #[test]
    fn delta_on_malformed_duration_still_returns_malformed_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: two weeks\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(1_800)),
        );

        // Replacing a typo with a measured number would destroy the
        // evidence that something was wrong.
        assert!(matches!(
            result.unwrap_err(),
            SetError::MutationCurrentValueMalformed { mode, ref field, .. }
                if mode == "delta" && field == "estimate"
        ));
        assert!(read_item(&root, "task-1").contains("estimate: two weeks"));
    }

    // ── Delta: duration on a derived field ───────────────────────────
    //
    // A schema default has no test of its own: a `duration` field cannot
    // declare a default at all, so there is nothing stamped in for a
    // delta to pick up.

    #[test]
    fn delta_on_a_rolled_up_duration_starts_from_zero_and_warns() {
        // Starting from the roll-up would freeze it into the file, where
        // it would go stale the next time a child changed.
        let (_directory, root) = setup_derived_duration_project();
        let config = load_test_config(&root);
        write_item(&root, "epic-1", "---\ntitle: Epic 1\nstatus: open\n---\n");
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nparent: epic-1\nrolled_up_effort: 2h\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("epic-1".to_owned()),
            "rolled_up_effort",
            SetOperation::Duration(DurationMode::Delta(1_800)),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
        // Nothing happens quietly: the hand-written value now competes
        // with the roll-up from `task-1`.
        assert!(
            outcome.mutation_caused_warning,
            "a manual value competing with a roll-up must surface, warnings: {:?}",
            outcome.warnings
        );
    }

    #[test]
    fn delta_on_a_computed_duration_starts_from_zero() {
        // `computed_effort` is `base_effort * 2` — 2h here, and still not
        // the delta's starting point.
        let (_directory, root) = setup_derived_duration_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nbase_effort: 1h\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "computed_effort",
            SetOperation::Duration(DurationMode::Delta(1_800)),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    }

    #[test]
    fn delta_on_a_pulled_duration_starts_from_zero() {
        // `pulled_effort` sums `base_effort` over `depends_on` — 3h here.
        let (_directory, root) = setup_derived_duration_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nbase_effort: 3h\n---\n",
        );
        write_item(
            &root,
            "task-2",
            "---\ntitle: Task 2\nstatus: open\ndepends_on: [task-1]\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-2".to_owned()),
            "pulled_effort",
            SetOperation::Duration(DurationMode::Delta(1_800)),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    }

    // ── Delta: date ──────────────────────────────────────────────────

    #[test]
    fn delta_on_date_adds_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // +1w
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(604_800)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-21");
    }

    #[test]
    fn delta_on_date_with_negative_subtracts_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // -3d
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-11");
    }

    #[test]
    fn delta_on_absent_date_field_returns_requires_existing() {
        // A date has no zero to count from. The asymmetry with duration
        // is deliberate and written down under `--delta` in the help.
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { .. })
        ));
    }

    #[test]
    fn delta_on_date_field_written_with_no_value_returns_requires_existing() {
        // A null is absent, not malformed — there is no value to be
        // invalid. Same rule as duration, different consequence.
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date:\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result.unwrap_err(),
            SetError::MutationRequiresExistingValue { mode, ref field }
                if mode == "delta" && field == "due_date"
        ));
    }

    #[test]
    fn date_delta_on_integer_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::ModeNotValidForFieldType { .. })
        ));
    }
}
