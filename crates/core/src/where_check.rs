//! Check the *operands* of a filter clause, not just its field names.
//!
//! [`crate::views_check`] resolves the field a `where:` clause names and
//! stops there, so `status=nonsense` on a `choice` field validates
//! cleanly, matches nothing, and renders an empty view with nothing to
//! explain it. This module answers the other half of the question: given
//! the field, could this operand ever match?
//!
//! Like `display_check`, it reports neutral [`ValueViolation`]s
//! rather than diagnostics, because three callers wrap them differently —
//! a view's `where:`, a metric row's per-row `where:`, and an ad-hoc
//! `workdown query --where`, which has no file to pin a diagnostic to at
//! all.
//!
//! Two kinds of "could never match" live here:
//!
//! - the field has a **closed option set** and the operand is not in it.
//!   Only two sets are closed by construction: a `choice`/`multichoice`
//!   field's declared `values` (coercion drops anything else) and, for
//!   the virtual `id`, the ids that exist. A `resource:`-backed field's
//!   section entries and a `link`/`links` field's item ids are policy,
//!   not fact — an unknown resource ref is a warning *by design* (the
//!   new hire assigned before resources.yaml caught up), a broken link
//!   stays on its item, and the evaluator matches both — so those sets
//!   are widened by the values items actually hold before the clause is
//!   judged;
//! - the field has a **type the operand cannot be read as** — a `date`
//!   that isn't a date, a `duration` that isn't a duration.
//!
//! # Staying honest about "never matches"
//!
//! Every rule here is derived from what [`crate::query::eval`] actually
//! does, not from what the operator is called. Two consequences that are
//! easy to get backwards:
//!
//! - `~` (contains) is **never** checked. On a collection it reads as
//!   substring-per-element, not whole-value membership, so `labels~end`
//!   legitimately matches `backend`. Only `=` and `!=` compare a whole
//!   value — and `in` / `not in` reduce to exactly those before they get
//!   here, so each member is checked individually with no special case.
//! - Ordering comparisons are checked only where the operand is *parsed*
//!   (numbers, durations, dates). There is no ordering on an option set,
//!   so an unknown `choice` value under `>` is not reported.
//!
//! A violation therefore means the clause is dead: the evaluator will
//! answer the same way for every item in the project, whatever they hold.

use std::collections::HashSet;

use crate::model::resources::Resources;
use crate::model::schema::{FieldTypeConfig, Schema};
use crate::model::FieldValue;
use crate::query::types::{Comparison, FieldReference, Operator, Predicate};
use crate::resources_check::validatable_fields;
use crate::store::Store;

/// How many option-set members a message lists before it gives up and
/// counts the rest. Long enough for a realistic `status` or `type`, short
/// enough that a 40-entry `people` section doesn't bury the message.
const MAX_LISTED_OPTIONS: usize = 8;

// ── Finding ──────────────────────────────────────────────────────────

/// One filter operand that cannot match any item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueViolation {
    /// The field as the clause spells it — `"status"`, or
    /// `"parent.status"` for a cross-relation reference.
    pub field: String,
    /// The offending operand, verbatim.
    pub value: String,
    /// What the operand would have had to be, phrased to complete the
    /// sentence "… is not `{expected}`" — e.g. `"one of: open, done"`,
    /// `"an existing work item id"`, `"a date (YYYY-MM-DD)"`.
    pub expected: String,
    /// A nudge for a mistake specific enough to name, appended to the
    /// message when present.
    pub hint: Option<String>,
}

impl ValueViolation {
    /// The full explanation, hint included. The diagnostic kinds store
    /// this rendered form so the wording lives in one place.
    pub fn detail(&self) -> String {
        let base = format!("'{}' is not {}", self.value, self.expected);
        match &self.hint {
            Some(hint) => format!("{base} — {hint}"),
            None => base,
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────

/// Check every comparison in a predicate. Returns one violation per
/// offending operand, in traversal order; an empty vector means every
/// operand could in principle match something.
pub fn check_predicate(
    predicate: &Predicate,
    schema: &Schema,
    resources: &Resources,
    store: &Store,
) -> Vec<ValueViolation> {
    let context = CheckContext {
        schema,
        store,
        resource_fields: validatable_fields(schema, resources)
            .into_iter()
            .map(|(field_name, field)| (field_name, field.entry_ids))
            .collect(),
    };
    let mut violations = Vec::new();
    walk(predicate, &context, &mut violations);
    violations
}

/// Shared state for one predicate walk.
struct CheckContext<'a> {
    schema: &'a Schema,
    store: &'a Store,
    /// Entry ids per `resource:`-backed field, for the fields worth
    /// checking at all. A field whose section is missing or empty is
    /// absent here — [`crate::resources_check`] reports that cause once
    /// against `schema.yaml`, and repeating it per clause would point at
    /// the wrong file.
    resource_fields: Vec<(&'a str, HashSet<&'a str>)>,
}

impl<'a> CheckContext<'a> {
    fn resource_entries(&self, field_name: &str) -> Option<&HashSet<&'a str>> {
        self.resource_fields
            .iter()
            .find(|(name, _)| *name == field_name)
            .map(|(_, entries)| entries)
    }
}

fn walk(predicate: &Predicate, context: &CheckContext, out: &mut Vec<ValueViolation>) {
    match predicate {
        Predicate::Comparison(comparison) => check_comparison(comparison, context, out),
        Predicate::And(branches) | Predicate::Or(branches) => {
            for branch in branches {
                walk(branch, context, out);
            }
        }
        Predicate::Not(inner) => walk(inner, context, out),
    }
}

// ── One comparison ───────────────────────────────────────────────────

fn check_comparison(
    comparison: &Comparison,
    context: &CheckContext,
    out: &mut Vec<ValueViolation>,
) {
    // The presence checks carry no operand, and a regex operand is a
    // pattern rather than a value. `in` / `not in` never reach the
    // evaluator or this walk — `query::parse` has already rewritten them
    // into per-member `=` / `!=` comparisons.
    match comparison.operator {
        Operator::IsSet | Operator::IsNotSet | Operator::Matches => return,
        Operator::Contains => return, // substring on every type — see module docs
        Operator::In | Operator::NotIn => return,
        _ => {}
    }

    // A cross-relation clause compares the *target* field's values, which
    // is the field the evaluator types it by; the message still quotes
    // the clause's own spelling.
    let (display_name, field_name) = match &comparison.field {
        FieldReference::Local(name) => (name.clone(), name.as_str()),
        FieldReference::Related { relation, field } => {
            (format!("{relation}.{field}"), field.as_str())
        }
    };

    let Some(check) = resolve_check(field_name, context) else {
        return;
    };

    let is_equality = matches!(comparison.operator, Operator::Equal | Operator::NotEqual);
    let value = comparison.operand.text();

    let violation = match check {
        // An option set answers "is this a member", a question the
        // ordering operators don't ask.
        ValueCheck::OptionSet { expected, members } if is_equality => {
            if members.contains(value) {
                None
            } else {
                Some(ValueViolation {
                    field: display_name,
                    value: value.to_owned(),
                    expected,
                    hint: membership_hint(field_name, value, &members),
                })
            }
        }
        ValueCheck::OptionSet { .. } => None,
        // Parsing is what the evaluator does before it compares, so the
        // check follows wherever it compares — ordering included.
        ValueCheck::Parses {
            expected,
            parses,
            equality_only,
        } => {
            if (equality_only && !is_equality) || parses(value) {
                None
            } else {
                Some(ValueViolation {
                    field: display_name,
                    value: value.to_owned(),
                    expected,
                    hint: None,
                })
            }
        }
    };

    out.extend(violation);
}

// ── What a field's operand is checked against ────────────────────────

/// The check that applies to one field, resolved from its schema
/// definition (or, for the virtual `id`, from the absence of one).
enum ValueCheck {
    /// The operand must be a member of a closed set.
    OptionSet {
        expected: String,
        members: HashSet<String>,
    },
    /// The operand must be readable as the field's type.
    Parses {
        expected: String,
        parses: fn(&str) -> bool,
        /// Types the evaluator only ever compares for equality
        /// (`boolean`, `color`); an ordering operator on them answers
        /// `false` whatever the operand says, which is an operator
        /// problem rather than a value one and is not reported here.
        equality_only: bool,
    },
}

fn resolve_check(field_name: &str, context: &CheckContext) -> Option<ValueCheck> {
    // The virtual `id` has no schema entry but the tightest option set of
    // all: the items that exist — by construction, nothing else can ever
    // be an item's id.
    if field_name == "id" {
        return Some(ValueCheck::OptionSet {
            expected: "an existing work item id".to_owned(),
            members: known_item_ids(context),
        });
    }

    // `resource:` is orthogonal to the field's type, so it is asked
    // first — a `string` field backed by a section has an option set
    // where a plain `string` has none.
    if let Some(entries) = context.resource_entries(field_name) {
        let mut members: HashSet<String> =
            entries.iter().map(|entry| (*entry).to_owned()).collect();
        members.extend(held_values(field_name, context.store));
        return Some(ValueCheck::OptionSet {
            expected: describe_option_set(&members),
            members,
        });
    }

    // An unknown field is `views_check`'s finding to report, not ours.
    let definition = context.schema.fields.get(field_name)?;

    Some(match &definition.type_config {
        FieldTypeConfig::Choice { values } | FieldTypeConfig::Multichoice { values } => {
            let members: HashSet<String> = values.iter().cloned().collect();
            ValueCheck::OptionSet {
                expected: describe_option_set(&members),
                members,
            }
        }
        // Item ids plus whatever the field actually holds: a broken
        // link is an error elsewhere, but the value stays on the item
        // and a clause naming it does match.
        FieldTypeConfig::Link { .. } | FieldTypeConfig::Links { .. } => {
            let mut members = known_item_ids(context);
            members.extend(held_values(field_name, context.store));
            ValueCheck::OptionSet {
                expected: "an existing work item id".to_owned(),
                members,
            }
        }
        FieldTypeConfig::Date => ValueCheck::Parses {
            expected: "a date (YYYY-MM-DD)".to_owned(),
            parses: |value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok(),
            equality_only: false,
        },
        FieldTypeConfig::Integer { .. } => ValueCheck::Parses {
            expected: "a whole number".to_owned(),
            parses: |value| value.parse::<i64>().is_ok(),
            equality_only: false,
        },
        FieldTypeConfig::Float { .. } => ValueCheck::Parses {
            expected: "a number".to_owned(),
            parses: |value| value.parse::<f64>().is_ok(),
            equality_only: false,
        },
        FieldTypeConfig::Duration { .. } => ValueCheck::Parses {
            expected: "a duration (e.g. '4h', '2d', '1w 2d')".to_owned(),
            parses: |value| crate::model::duration::parse_duration(value).is_ok(),
            equality_only: false,
        },
        FieldTypeConfig::Boolean => ValueCheck::Parses {
            expected: "'true' or 'false'".to_owned(),
            parses: |value| matches!(value, "true" | "false"),
            equality_only: true,
        },
        FieldTypeConfig::Color => ValueCheck::Parses {
            expected: "a hex color or palette name".to_owned(),
            parses: |value| crate::model::color::parse_color(value).is_ok(),
            equality_only: true,
        },
        // Free-form text: any operand could match something.
        FieldTypeConfig::String { .. } | FieldTypeConfig::List => return None,
    })
}

fn known_item_ids(context: &CheckContext) -> HashSet<String> {
    context
        .store
        .all_items()
        .map(|item| item.id.as_str().to_owned())
        .collect()
}

/// Every whole value items currently hold in `field_name` — the strings
/// equality actually compares against. Values outside a field's declared
/// set are legal data (an unknown resource ref warns, a broken link
/// errors — both stay on their item and both match), so a set that
/// ignored them would call clauses dead that match today.
fn held_values(field_name: &str, store: &Store) -> Vec<String> {
    let mut values = Vec::new();
    for item in store.all_items() {
        match item.fields.get(field_name) {
            Some(
                FieldValue::String(value) | FieldValue::Choice(value) | FieldValue::Color(value),
            ) => values.push(value.clone()),
            Some(FieldValue::Link(target)) => values.push(target.as_str().to_owned()),
            Some(FieldValue::Multichoice(members) | FieldValue::List(members)) => {
                values.extend(members.iter().cloned());
            }
            Some(FieldValue::Links(targets)) => {
                values.extend(targets.iter().map(|target| target.as_str().to_owned()));
            }
            _ => {}
        }
    }
    values
}

// ── Message shaping ──────────────────────────────────────────────────

/// Phrase an option set for the message, listing members in sorted order
/// and counting the tail once the list gets long. Sorted rather than
/// declaration-ordered because a `HashSet` has no order to preserve and
/// an alphabetical list is the one a reader can scan.
fn describe_option_set(members: &HashSet<String>) -> String {
    if members.is_empty() {
        return "one of this field's values (it declares none)".to_owned();
    }
    let mut sorted: Vec<&str> = members.iter().map(String::as_str).collect();
    sorted.sort_unstable();

    if sorted.len() <= MAX_LISTED_OPTIONS {
        return format!("one of: {}", sorted.join(", "));
    }
    let shown = sorted[..MAX_LISTED_OPTIONS].join(", ");
    let rest = sorted.len() - MAX_LISTED_OPTIONS;
    format!("one of: {shown}, … ({rest} more)")
}

/// Recognize the one mistake worth naming: a comma-joined operand whose
/// every part *is* a valid member. Before the explicit `in` operator
/// landed, `type=milestone,epic` was an implicit membership test; it now
/// compares against the literal string `"milestone,epic"`, so a filter
/// that used to work silently matches nothing.
fn membership_hint(field_name: &str, value: &str, members: &HashSet<String>) -> Option<String> {
    if !value.contains(',') {
        return None;
    }
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() < 2 || !parts.iter().all(|part| members.contains(*part)) {
        return None;
    }
    Some(format!(
        "did you mean '{field_name} in {}'?",
        parts.join(",")
    ))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use indexmap::IndexMap;

    use crate::model::resources::{ResourceEntry, Resources};
    use crate::model::schema::FieldDefinition;
    use crate::query::parse::parse_where;

    // ── Fixtures ────────────────────────────────────────────────

    fn schema() -> Schema {
        let mut fields: IndexMap<String, FieldDefinition> = IndexMap::new();
        let mut insert = |name: &str, config: FieldTypeConfig| {
            fields.insert(name.to_owned(), FieldDefinition::new(config));
        };
        insert("title", FieldTypeConfig::String { pattern: None });
        insert(
            "status",
            FieldTypeConfig::Choice {
                values: vec!["open".into(), "in_progress".into(), "done".into()],
            },
        );
        insert(
            "labels",
            FieldTypeConfig::Multichoice {
                values: vec!["backend".into(), "frontend".into()],
            },
        );
        insert(
            "type",
            FieldTypeConfig::Choice {
                values: vec!["milestone".into(), "epic".into(), "task".into()],
            },
        );
        insert(
            "parent",
            FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            },
        );
        insert(
            "depends_on",
            FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: None,
            },
        );
        insert("due_date", FieldTypeConfig::Date);
        insert(
            "points",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
        );
        insert(
            "weight",
            FieldTypeConfig::Float {
                min: None,
                max: None,
            },
        );
        insert(
            "estimate",
            FieldTypeConfig::Duration {
                min: None,
                max: None,
            },
        );
        insert("active", FieldTypeConfig::Boolean);
        insert("tint", FieldTypeConfig::Color);
        insert("tags", FieldTypeConfig::List);

        let mut assignee = FieldDefinition::new(FieldTypeConfig::String { pattern: None });
        assignee.resource = Some("people".to_owned());
        fields.insert("assignee".to_owned(), assignee);

        let mut reviewers = FieldDefinition::new(FieldTypeConfig::List);
        reviewers.resource = Some("nobody".to_owned());
        fields.insert("reviewers".to_owned(), reviewers);

        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    /// A `people` section with two entries, and an empty `nobody`
    /// section — the "declared but unpopulated" case.
    fn resources() -> Resources {
        let mut sections = IndexMap::new();
        sections.insert(
            "people".to_owned(),
            vec![
                ResourceEntry {
                    id: "alice".to_owned(),
                    name: None,
                },
                ResourceEntry {
                    id: "bob".to_owned(),
                    name: None,
                },
            ],
        );
        sections.insert("nobody".to_owned(), Vec::new());
        Resources {
            sections,
            constants: IndexMap::new(),
            document_loaded: true,
        }
    }

    /// A store holding `epic-1` and `task-a`.
    fn store(schema: &Schema) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        for id in ["epic-1", "task-a"] {
            std::fs::write(dir.path().join(format!("{id}.md")), "---\n---\n").unwrap();
        }
        let store = Store::load(dir.path(), schema).unwrap();
        (dir, store)
    }

    /// Check one clause string, returning its violations.
    fn check(clause: &str) -> Vec<ValueViolation> {
        let schema = schema();
        let (_dir, store) = store(&schema);
        let predicate = parse_where(clause).expect("clause parses");
        check_predicate(&predicate, &schema, &resources(), &store)
    }

    /// Check one clause against a store built from explicit items —
    /// `(id, frontmatter)` pairs — for the cases where what items *hold*
    /// matters, not just what the schema declares.
    fn check_with_items(clause: &str, items: &[(&str, &str)]) -> Vec<ValueViolation> {
        let schema = schema();
        let dir = tempfile::tempdir().unwrap();
        for (id, frontmatter) in items {
            std::fs::write(
                dir.path().join(format!("{id}.md")),
                format!("---\n{frontmatter}---\n"),
            )
            .unwrap();
        }
        let store = Store::load(dir.path(), &schema).unwrap();
        let predicate = parse_where(clause).expect("clause parses");
        check_predicate(&predicate, &schema, &resources(), &store)
    }

    /// Assert a clause is clean.
    fn assert_clean(clause: &str) {
        let violations = check(clause);
        assert!(violations.is_empty(), "{clause} → {violations:?}");
    }

    /// Assert a clause produces exactly one violation, and return it.
    fn assert_one(clause: &str) -> ValueViolation {
        let mut violations = check(clause);
        assert_eq!(violations.len(), 1, "{clause} → {violations:?}");
        violations.remove(0)
    }

    // ── Option sets ─────────────────────────────────────────────

    #[test]
    fn choice_value_outside_the_declared_set_is_reported() {
        let violation = assert_one("status=nonsense");
        assert_eq!(violation.field, "status");
        assert_eq!(violation.value, "nonsense");
        assert_eq!(violation.expected, "one of: done, in_progress, open");
        assert_eq!(
            violation.detail(),
            "'nonsense' is not one of: done, in_progress, open"
        );
    }

    #[test]
    fn declared_choice_values_are_clean() {
        for clause in ["status=open", "status!=done", "labels=backend"] {
            assert_clean(clause);
        }
    }

    #[test]
    fn negated_equality_is_checked_too() {
        // `status != nonsense` matches *everything*, which is no more
        // what the author meant than matching nothing.
        let violation = assert_one("status!=nonsense");
        assert_eq!(violation.value, "nonsense");
    }

    #[test]
    fn multichoice_value_outside_the_declared_set_is_reported() {
        let violation = assert_one("labels=sideways");
        assert_eq!(violation.field, "labels");
    }

    #[test]
    fn resource_backed_field_checks_against_section_entries() {
        assert_clean("assignee=alice");
        let violation = assert_one("assignee=carol");
        assert_eq!(violation.expected, "one of: alice, bob");
    }

    #[test]
    fn resource_field_with_an_empty_section_stays_quiet() {
        // `resources_check` reports the unpopulated section once against
        // `schema.yaml`; repeating it per clause would point at the
        // wrong file. Same rule the per-item value check follows.
        assert_clean("reviewers=anyone");
    }

    #[test]
    fn link_operand_checks_against_item_ids() {
        assert_clean("parent=epic-1");
        let violation = assert_one("parent=epic-2");
        assert_eq!(violation.expected, "an existing work item id");
    }

    #[test]
    fn links_operand_checks_against_item_ids() {
        assert_clean("depends_on=task-a");
        assert_eq!(assert_one("depends_on=task-z").field, "depends_on");
    }

    #[test]
    fn a_resource_value_an_item_holds_is_clean() {
        // `assignee: carol` is legal data — an unknown resource ref is a
        // warning by design — and the evaluator matches it, so the
        // clause is not dead and must not be reported.
        let items = [("onboard-carol", "assignee: carol\n")];
        let violations = check_with_items("assignee=carol", &items);
        assert!(violations.is_empty(), "got: {violations:?}");

        // A value neither declared nor held is still a typo worth
        // naming — and the option list now includes the held value.
        let mut violations = check_with_items("assignee=carrol", &items);
        assert_eq!(violations.len(), 1, "got: {violations:?}");
        assert_eq!(violations.remove(0).expected, "one of: alice, bob, carol");
    }

    #[test]
    fn a_broken_link_an_item_holds_is_clean() {
        // The broken reference is reported where it lives (the item);
        // filtering for it *finds* that item, which is exactly how one
        // hunts the breakage down.
        let items = [("task-a", "parent: ghost\n")];
        let violations = check_with_items("parent=ghost", &items);
        assert!(violations.is_empty(), "got: {violations:?}");
        assert_eq!(check_with_items("parent=phantom", &items).len(), 1);
    }

    #[test]
    fn the_virtual_id_set_is_not_widened_by_held_values() {
        // Nothing can *be* an id except the items that exist — a broken
        // link target held in `parent` is still not anyone's id.
        let items = [("task-a", "parent: ghost\n")];
        assert_eq!(check_with_items("id=ghost", &items).len(), 1);
    }

    #[test]
    fn virtual_id_checks_against_item_ids() {
        assert_clean("id=task-a");
        assert_eq!(assert_one("id=ghost").field, "id");
    }

    #[test]
    fn membership_reports_each_bad_member_once() {
        // `in` is desugared before it reaches the walk, so members are
        // checked individually with no special case.
        let violations = check("status in open,nonsense,alsobad");
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert_eq!(violations[0].value, "nonsense");
        assert_eq!(violations[1].value, "alsobad");
    }

    #[test]
    fn negated_membership_is_checked_the_same_way() {
        assert_eq!(assert_one("status not in nonsense").value, "nonsense");
    }

    // ── The stale implicit-IN filter ────────────────────────────

    #[test]
    fn comma_joined_choice_value_suggests_the_in_operator() {
        // The regression `explicit-in-operator` introduced: this used to
        // be a membership test and is now a literal string comparison.
        let violation = assert_one("type=milestone,epic");
        assert_eq!(violation.value, "milestone,epic");
        assert_eq!(
            violation.hint.as_deref(),
            Some("did you mean 'type in milestone,epic'?")
        );
        assert!(violation
            .detail()
            .ends_with("did you mean 'type in milestone,epic'?"));
    }

    #[test]
    fn comma_joined_value_with_an_unknown_part_gets_no_hint() {
        // Suggesting `in` here would just move the problem.
        let violation = assert_one("type=milestone,nonsense");
        assert_eq!(violation.hint, None);
    }

    // ── Parse checks ────────────────────────────────────────────

    #[test]
    fn unparseable_date_is_reported_on_equality_and_ordering() {
        for clause in [
            "due_date=yesterday",
            "due_date>yesterday",
            "due_date<=soon",
            "due_date>=2026-13-45",
        ] {
            let violation = assert_one(clause);
            assert_eq!(violation.expected, "a date (YYYY-MM-DD)", "{clause}");
        }
        assert_clean("due_date>2026-03-01");
    }

    #[test]
    fn unparseable_numbers_are_reported() {
        assert_eq!(assert_one("points>many").expected, "a whole number");
        assert_eq!(assert_one("weight<heavy").expected, "a number");
        assert_clean("points>3");
        assert_clean("weight<1.5");
        // An integer field rejects a fractional operand, as the
        // evaluator's `parse::<i64>` does.
        assert_eq!(assert_one("points=1.5").expected, "a whole number");
    }

    #[test]
    fn unparseable_duration_is_reported() {
        assert_clean("estimate>1w 2d");
        assert_eq!(
            assert_one("estimate>ages").expected,
            "a duration (e.g. '4h', '2d', '1w 2d')"
        );
    }

    #[test]
    fn boolean_and_color_are_checked_on_equality_only() {
        assert_clean("active=true");
        assert_eq!(assert_one("active=maybe").expected, "'true' or 'false'");

        assert_clean("tint=red");
        assert_clean("tint=#ef4444");
        assert_eq!(
            assert_one("tint=chartreuse").expected,
            "a hex color or palette name"
        );

        // Ordering on these types answers `false` whatever the operand
        // says — an operator problem, not a value one, so not ours.
        assert_clean("active>maybe");
    }

    // ── What is deliberately not checked ────────────────────────

    #[test]
    fn regex_operands_are_never_checked() {
        assert_clean("status/^nonsense$/");
        assert_clean("title/^fix-/i");
    }

    #[test]
    fn contains_is_never_checked() {
        // `~` reads as substring-per-element on every type, so a partial
        // value is exactly what it is for: `labels~end` matches
        // `backend`.
        for clause in ["labels~end", "title~login", "depends_on~task", "status~ope"] {
            assert_clean(clause);
        }
    }

    #[test]
    fn presence_checks_carry_no_operand() {
        assert_clean("assignee?");
        assert_clean("!assignee?");
    }

    #[test]
    fn free_form_fields_have_nothing_to_check() {
        assert_clean("title=anything at all");
        assert_clean("tags=whatever");
    }

    #[test]
    fn unknown_fields_are_left_to_views_check() {
        // Judging an operand against a field that doesn't exist would
        // stack a second complaint on one cause.
        assert_clean("nonexistent=whatever");
    }

    #[test]
    fn ordering_on_an_option_set_is_not_reported() {
        // There is no ordering on a category, so `>` says nothing about
        // membership; the evaluator compares lexicographically and the
        // clause is odd but not provably dead.
        assert_clean("status>nonsense");
    }

    // ── Traversal ───────────────────────────────────────────────

    #[test]
    fn every_branch_of_a_boolean_tree_is_walked() {
        // A clause carries one comparison; the `And` is how a `where:`
        // list and `query --where` combine several of them.
        let schema = schema();
        let (_dir, store) = store(&schema);
        let predicate = Predicate::And(vec![
            parse_where("status=nonsense").unwrap(),
            parse_where("parent=epic-2").unwrap(),
        ]);
        let violations = check_predicate(&predicate, &schema, &resources(), &store);
        assert_eq!(violations.len(), 2, "{violations:?}");
    }

    #[test]
    fn a_negated_clause_is_walked_through() {
        let schema = schema();
        let (_dir, store) = store(&schema);
        let predicate = Predicate::Not(Box::new(parse_where("status=nonsense").unwrap()));
        let violations = check_predicate(&predicate, &schema, &resources(), &store);
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn cross_relation_clauses_are_checked_on_the_target_field() {
        let violation = assert_one("parent.status=nonsense");
        // The message quotes the clause's own spelling…
        assert_eq!(violation.field, "parent.status");
        // …while the option set comes from the target field.
        assert_eq!(violation.expected, "one of: done, in_progress, open");
    }

    // ── Message shaping ─────────────────────────────────────────

    #[test]
    fn long_option_sets_are_truncated_with_a_count() {
        let members: HashSet<String> = (1..=12).map(|index| format!("value-{index:02}")).collect();
        let described = describe_option_set(&members);
        assert!(described.starts_with("one of: value-01, "), "{described}");
        assert!(described.ends_with("… (4 more)"), "{described}");
    }
}
