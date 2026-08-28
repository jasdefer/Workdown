//! Effort-timer wire contracts — the shapes `GET /api/timer`,
//! `POST /api/timer/start`, `POST /api/timer/stop` and
//! `POST /api/timer/break/end` exchange with the web app, plus the
//! rules both sides must agree on: which field the timer writes to
//! (resolved against the schema), how a stopped session's elapsed time
//! rounds into the write, and the fixed pomodoro interval lengths.
//!
//! Like [`crate::mutation_data`], these carry a `ts_rs` derive so
//! `cargo xtask gen-types` emits matching TypeScript. The timer's state
//! machine itself lives in the server crate — a running timer belongs
//! to the running app, never to a file — but the vocabulary lives here,
//! next to the write path a stop feeds ([`crate::operations::set`]).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::duration::parse_duration;
use crate::model::schema::{FieldType, Schema};
use crate::model::WorkItemId;
use crate::operations::set::{current_value, SetOutcome};
use crate::parser;

/// Round a stopped session's elapsed seconds into the seconds actually
/// written: nearest whole minute, thirty seconds rounds up. Zero means
/// the stop writes nothing at all.
///
/// Mirrored as the same one-line rule in the UI (`timerMath.ts`) so the
/// projected write shown while the timer runs and the write a stop
/// performs can never disagree.
pub fn rounded_write_seconds(elapsed_seconds: u64) -> i64 {
    let minutes = elapsed_seconds.saturating_add(30) / 60;
    i64::try_from(minutes.saturating_mul(60)).unwrap_or(i64::MAX)
}

/// The hand-written effort value the projected write starts from: the
/// field's value in the item file's own frontmatter, parsed as canonical
/// seconds. `None` means absent — including a blank or null key, exactly
/// as the delta write's own absence rule (`current_value`) decides.
///
/// Deliberately read from the file rather than from a loaded store: the
/// store's derive pass writes rolled-up and computed values into item
/// fields, where they are indistinguishable from hand-written ones — and
/// the delta a stop performs starts from zero on those, so the
/// projection must too. A value that is present but malformed also reads
/// as absent here; the stop itself refuses to write over it and reports
/// why.
pub fn hand_written_duration_seconds(item_file: &Path, field: &str) -> Option<i64> {
    let content = std::fs::read_to_string(item_file).ok()?;
    let (frontmatter, _body) = parser::split_frontmatter(&content, item_file).ok()?;
    let value = current_value(&frontmatter, field)?;
    parse_duration(value.as_str()?).ok()
}

/// The pomodoro interval lengths, in seconds. Deliberately not
/// configurable — a personal working habit, not project policy — and
/// deliberately here, beside the rounding rule: the browser never
/// hardcodes them, it reads the running phase's length off the wire.
pub const POMODORO_WORK_SECONDS: u64 = 25 * 60;
pub const POMODORO_BREAK_SECONDS: u64 = 5 * 60;

/// The two ways the timer runs: the open-ended stopwatch, or pomodoro —
/// a counted-down work interval followed by a break. The mode never
/// changes what a stop records (measured work time, rounded); it
/// changes the pacing and the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum TimerMode {
    Stopwatch,
    Pomodoro,
}

/// What `defaults.effort_field` in `config.yaml` resolves to, carried on
/// every timer response so the item view's timer slot can render the
/// right one of its states: no timer plus a hint naming the key
/// (`unconfigured`), a visible problem instead of a silent absence
/// (`invalid`), or the field the timer writes to (`ready`).
///
/// No other part of `config.yaml` reaches the UI — this enum is the
/// whole exposure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EffortFieldState {
    /// The key is unset — the normal "no timer" state, not a problem.
    Unconfigured,
    /// The key names something no timer can write to. `problem` is a
    /// human-readable clause; the UI wraps it with the key name and the
    /// restart hint (config is read once at server start).
    Invalid {
        field: String,
        problem: String,
    },
    Ready {
        field: String,
    },
}

impl EffortFieldState {
    /// Resolve `defaults.effort_field` against the schema, mirroring the
    /// rules `config_check` warns on: the key must name an existing
    /// schema field of type `duration`, and the virtual `id` is rejected
    /// by name before the schema is consulted.
    pub fn resolve(effort_field: Option<&str>, schema: &Schema) -> Self {
        let Some(field) = effort_field else {
            return Self::Unconfigured;
        };
        let invalid = |problem: String| Self::Invalid {
            field: field.to_owned(),
            problem,
        };

        if field == "id" {
            return invalid("the virtual 'id' is unique per item and cannot carry effort".into());
        }
        match schema.fields.get(field) {
            None => invalid(format!("schema.yaml defines no field named '{field}'")),
            Some(definition) if definition.field_type() != FieldType::Duration => invalid(format!(
                "field '{field}' has type {}, but effort needs a duration",
                definition.field_type()
            )),
            Some(_) => Self::Ready {
                field: field.to_owned(),
            },
        }
    }
}

/// What the timer is doing right now — one of three shapes, each
/// carrying only the fields that exist in that phase. A break is not a
/// work session with blanks in it: it times no item and will write
/// nothing, so it gets its own shape instead of a shared block of
/// sometimes-meaningless fields.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum TimerPhase {
    /// No timer runs.
    Idle,
    /// An item is being timed — an open-ended stopwatch session or a
    /// pomodoro work interval, distinguished by `mode`.
    Work {
        item_id: WorkItemId,
        /// Wall-clock start, milliseconds since the Unix epoch. Display
        /// only — the browser formats it as a local time; elapsed time
        /// never derives from it client-side.
        #[ts(type = "number")]
        started_at_ms: i64,
        /// Elapsed seconds, computed server-side at response time
        /// (clamped at zero against backwards clock jumps). The UI
        /// ticks locally from this anchor — "the server said X seconds,
        /// Y moments ago" — so a wrong browser clock cannot skew the
        /// display.
        #[ts(type = "number")]
        elapsed_seconds: u64,
        /// The item's hand-written effort value in canonical seconds;
        /// `null` when absent (a derived value — rolled up, computed —
        /// counts as absent too, exactly as the delta write does). The
        /// basis of the projected write: stop lands `effort_before +
        /// rounded elapsed`.
        #[ts(type = "number | null")]
        effort_before_seconds: Option<i64>,
        mode: TimerMode,
        /// The countdown's target: [`POMODORO_WORK_SECONDS`] in
        /// pomodoro mode, `null` on the stopwatch, which counts toward
        /// nothing.
        #[ts(type = "number | null")]
        phase_length_seconds: Option<u64>,
    },
    /// A pomodoro break — counted down but never recorded.
    Break {
        /// The item whose stop began this break: the default for the
        /// next interval, and the one item a start needs no repeated
        /// roll-up confirmation for.
        followed_item: WorkItemId,
        #[ts(type = "number")]
        started_at_ms: i64,
        #[ts(type = "number")]
        elapsed_seconds: u64,
        /// The countdown's target: [`POMODORO_BREAK_SECONDS`].
        #[ts(type = "number")]
        phase_length_seconds: u64,
    },
}

/// The timer's whole client-visible state — `GET /api/timer`, and the
/// payload of a successful start. Every tab holds one of these and
/// refetches it on the timer-named live-update ping.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TimerState {
    pub effort_field: EffortFieldState,
    pub phase: TimerPhase,
    /// The sticky mode: what the last started session ran as, stopwatch
    /// until a pomodoro session has ever started. Always present, even
    /// when idle — it preselects the split button. While a work phase
    /// runs it always equals that phase's `mode` (every start sets
    /// both); stated twice so neither reader relies on the invariant.
    pub last_mode: TimerMode,
}

/// A request to start the timer. `confirmed` is the second leg of the
/// roll-up confirmation round trip: start on an item whose effort field
/// aggregates and that has children answers `needs_confirmation`, the
/// app shows the dialog, and sends the same request again with
/// `confirmed: true`.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct StartTimer {
    pub item: String,
    /// Required, no default: the only client is our own UI, which
    /// always knows which button was pressed, and an explicit request
    /// reads unambiguously in tests and logs.
    pub mode: TimerMode,
    #[serde(default)]
    pub confirmed: bool,
}

/// Start's successful reply is one of two shapes — a normal fork in the
/// flow, not an error, so it travels as a typed outcome inside the
/// envelope's `data` rather than as a third envelope kind or as error
/// text the browser would have to parse.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum TimerStartOutcome {
    Started {
        timer: TimerState,
    },
    /// The item's effort rolls up from children; starting would record
    /// a hand-written value that overrides the roll-up. The app names
    /// the override in a confirmation dialog before anything starts.
    NeedsConfirmation,
}

/// The result of a successful stop — everything the toast reports.
/// Warnings from the post-write reload ride in the envelope's
/// `diagnostics`, exactly as field mutations' do.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TimerStopResult {
    pub item_id: WorkItemId,
    /// The effort field the stop wrote (or would have written) to.
    pub field: String,
    /// Raw measured seconds, before rounding.
    #[ts(type = "number")]
    pub elapsed_seconds: u64,
    /// `null` when the session rounded to zero minutes and the stop
    /// wrote nothing — the toast says so instead of staying silent.
    pub write: Option<TimerWrite>,
}

/// The write a stop performed, projected for the toast: the amount
/// added, the value before and after, and what undo needs — the raw
/// before-value to set back (`previous_value`), or absence, meaning
/// undo unsets the field.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct TimerWrite {
    /// Minutes-rounded seconds actually added; always positive.
    #[ts(type = "number")]
    pub added_seconds: i64,
    /// The frontmatter value before the write, verbatim — undo replaces
    /// the field with exactly this (preserving the user's spelling), or
    /// unsets it when `null`.
    #[ts(type = "unknown")]
    pub previous_value: Option<serde_yaml::Value>,
    #[ts(type = "number | null")]
    pub previous_seconds: Option<i64>,
    #[ts(type = "number")]
    pub new_seconds: i64,
    /// `true` when this write introduced a diagnostic that wasn't
    /// present before (e.g. a hand-written value now competing with a
    /// roll-up).
    pub mutation_caused_warning: bool,
    pub info_messages: Vec<String>,
}

impl TimerWrite {
    /// Project a duration-delta [`SetOutcome`] into the toast's numbers.
    /// The outcome's previous value is a duration string by construction
    /// (the delta path's precondition admits nothing else), so parsing
    /// cannot fail on a real outcome; `None` means the field was absent
    /// and the write started from zero.
    pub fn from_outcome(added_seconds: i64, outcome: &SetOutcome) -> Self {
        let previous_seconds = outcome.previous_value.as_ref().map(|value| {
            let string = value
                .as_str()
                .expect("delta precondition ensures a duration string");
            parse_duration(string).expect("delta precondition ensures a parseable duration")
        });
        Self {
            added_seconds,
            previous_value: outcome.previous_value.clone(),
            previous_seconds,
            new_seconds: previous_seconds.unwrap_or(0).saturating_add(added_seconds),
            mutation_caused_warning: outcome.mutation_caused_warning,
            info_messages: outcome.info_messages.clone(),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use indexmap::IndexMap;

    // ── rounded_write_seconds: the boundary table of the rule ───────

    #[test]
    fn under_half_a_minute_writes_nothing() {
        assert_eq!(rounded_write_seconds(0), 0);
        assert_eq!(rounded_write_seconds(29), 0);
    }

    #[test]
    fn thirty_seconds_rounds_up_to_one_minute() {
        assert_eq!(rounded_write_seconds(30), 60);
        assert_eq!(rounded_write_seconds(89), 60);
    }

    #[test]
    fn ninety_seconds_rounds_up_to_two_minutes() {
        assert_eq!(rounded_write_seconds(90), 120);
    }

    #[test]
    fn exact_minutes_stay_exact() {
        assert_eq!(rounded_write_seconds(60), 60);
        assert_eq!(rounded_write_seconds(3600), 3600);
    }

    #[test]
    fn absurd_elapsed_saturates_instead_of_panicking() {
        assert_eq!(rounded_write_seconds(u64::MAX), i64::MAX);
    }

    // ── hand_written_duration_seconds ────────────────────────────────

    /// One throwaway item file with the given content; the directory
    /// handle keeps the file alive for the test's duration.
    fn item_file(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("task-1.md");
        std::fs::write(&path, content).expect("write item file");
        (directory, path)
    }

    #[test]
    fn hand_written_value_parses_to_canonical_seconds() {
        let (_directory, path) = item_file("---\ntitle: Task\neffort: 2h\n---\n");
        assert_eq!(hand_written_duration_seconds(&path, "effort"), Some(7200));
    }

    #[test]
    fn absent_blank_and_malformed_values_read_as_absent() {
        let (_directory, path) =
            item_file("---\ntitle: Task\nblank:\nmalformed: not-a-duration\n---\n");
        assert_eq!(hand_written_duration_seconds(&path, "missing"), None);
        assert_eq!(hand_written_duration_seconds(&path, "blank"), None);
        assert_eq!(hand_written_duration_seconds(&path, "malformed"), None);
    }

    #[test]
    fn unreadable_file_reads_as_absent() {
        let (directory, _path) = item_file("---\ntitle: Task\n---\n");
        let gone = directory.path().join("no-such-item.md");
        assert_eq!(hand_written_duration_seconds(&gone, "effort"), None);
    }

    // ── EffortFieldState::resolve ────────────────────────────────────

    fn schema_with(fields: Vec<(&str, FieldTypeConfig)>) -> Schema {
        let mut map = IndexMap::new();
        for (name, cfg) in fields {
            map.insert(name.to_owned(), FieldDefinition::new(cfg));
        }
        Schema::new(map, vec![])
    }

    fn duration_field() -> FieldTypeConfig {
        FieldTypeConfig::Duration {
            min: None,
            max: None,
        }
    }

    #[test]
    fn unset_key_is_unconfigured() {
        let schema = schema_with(vec![("effort", duration_field())]);
        assert_eq!(
            EffortFieldState::resolve(None, &schema),
            EffortFieldState::Unconfigured
        );
    }

    #[test]
    fn duration_field_is_ready() {
        let schema = schema_with(vec![("effort", duration_field())]);
        assert_eq!(
            EffortFieldState::resolve(Some("effort"), &schema),
            EffortFieldState::Ready {
                field: "effort".into()
            }
        );
    }

    #[test]
    fn unknown_field_is_invalid() {
        let schema = schema_with(vec![("effort", duration_field())]);
        let EffortFieldState::Invalid { field, problem } =
            EffortFieldState::resolve(Some("efort"), &schema)
        else {
            panic!("expected Invalid");
        };
        assert_eq!(field, "efort");
        assert!(problem.contains("no field named 'efort'"), "{problem}");
    }

    #[test]
    fn empty_string_is_a_typo_not_a_second_unset() {
        let schema = schema_with(vec![("effort", duration_field())]);
        assert!(matches!(
            EffortFieldState::resolve(Some(""), &schema),
            EffortFieldState::Invalid { .. }
        ));
    }

    #[test]
    fn non_duration_field_is_invalid() {
        let schema = schema_with(vec![(
            "status",
            FieldTypeConfig::Choice {
                values: vec!["open".into()],
            },
        )]);
        let EffortFieldState::Invalid { problem, .. } =
            EffortFieldState::resolve(Some("status"), &schema)
        else {
            panic!("expected Invalid");
        };
        assert!(problem.contains("needs a duration"), "{problem}");
    }

    #[test]
    fn virtual_id_is_rejected_by_name() {
        // Even a schema that declares its own `id` field never wins the
        // role — same verdict the structural view slots reach.
        let schema = schema_with(vec![("id", duration_field())]);
        assert!(matches!(
            EffortFieldState::resolve(Some("id"), &schema),
            EffortFieldState::Invalid { .. }
        ));
    }

    // ── TimerWrite::from_outcome ─────────────────────────────────────

    fn outcome(previous: Option<&str>, new: &str) -> SetOutcome {
        SetOutcome {
            path: std::path::PathBuf::from("workdown-items/task-a.md"),
            previous_value: previous.map(|value| serde_yaml::Value::String(value.to_owned())),
            new_value: Some(serde_yaml::Value::String(new.to_owned())),
            warnings: vec![],
            info_messages: vec![],
            mutation_caused_warning: false,
        }
    }

    #[test]
    fn absent_previous_value_starts_from_zero() {
        let write = TimerWrite::from_outcome(120, &outcome(None, "2min"));
        assert_eq!(write.previous_value, None);
        assert_eq!(write.previous_seconds, None);
        assert_eq!(write.new_seconds, 120);
        assert_eq!(write.added_seconds, 120);
    }

    #[test]
    fn present_previous_value_carries_verbatim_and_parsed() {
        let write = TimerWrite::from_outcome(2400, &outcome(Some("2h"), "2h 40min"));
        assert_eq!(
            write.previous_value,
            Some(serde_yaml::Value::String("2h".into()))
        );
        assert_eq!(write.previous_seconds, Some(7200));
        assert_eq!(write.new_seconds, 9600);
    }
}
