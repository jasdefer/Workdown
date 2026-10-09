//! The rule grammar as the rules editor sees it: a rule's `match`,
//! `require` and `count` as flat rows, and the rows back as the YAML
//! the parser reads.
//!
//! The editor's unit is a row — a field reference, a test and a value —
//! because the parser's own shape (a scalar for equality, a list for
//! membership, an object of operators, a quantifier wrapping another
//! condition) is what someone editing by hand has to know and what the
//! form is there to spare them. Two rows on one reference merge into the
//! parser's operator object on the way back
//! (`workdown-items/schema-rules-editor.md`, decision 7); a lone
//! equality or `required` is written in its shorthand (decision 9).
//!
//! Both directions are built from the parsed model's own types with
//! exhaustive matches, so a condition operator or an assertion operator
//! added to [`crate::model::condition`] or [`crate::model::assertion`]
//! fails to compile here until the rows can carry it. That is the
//! guarantee the editor rests on: there is no second copy of the
//! grammar anywhere (decision 12).
//!
//! What the rows cannot hold is reported, not approximated: a
//! quantifier nested inside a quantifier (the evaluator ignores it) and
//! `required: false`, which has no meaning the editor could show. Such
//! a rule is served without a shape and displayed as text (decision
//! 11).
//!
//! Every type here carries a `ts_rs` derive; `cargo xtask gen-types`
//! emits the TypeScript. A new type needs its `exports.add` line in
//! `crates/core/examples/gen_types.rs`.

use serde::{Deserialize, Serialize};

use crate::model::assertion::{Assertion, AssertionOperator};
use crate::model::condition::{Condition, ConditionOperator, ConditionValue, NegationValue};
use crate::model::rule::{CountConstraint, Rule};

// ── Wire types ────────────────────────────────────────────────────────

/// A rule's body as rows: what the editor shows and sends back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct RuleShape {
    /// The `match:` block, one row per test. Rows on the same reference
    /// are one condition; rows under the same quantifier are that
    /// quantifier's inner condition.
    pub match_rows: Vec<MatchRow>,
    /// The `require:` block, one row per assertion operator. Rows on
    /// the same reference are one assertion.
    pub require_rows: Vec<RequireRow>,
    /// The `count:` block, when the rule has one.
    pub count: Option<CountConstraint>,
}

/// One test in `match:`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MatchRow {
    /// The field reference the test applies to: a field name, or a
    /// relation and a field as `children.status`, or a bare relation.
    pub reference: String,
    /// Set when the test applies through a one-to-many relation; the
    /// test is then the quantifier's inner condition.
    pub quantifier: Option<Quantifier>,
    pub test: ScalarTest,
}

/// How a test over related items is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Quantifier {
    /// Every related item satisfies the test (vacuously true on none).
    All,
    /// At least one related item satisfies the test.
    Any,
    /// No related item satisfies the test.
    #[serde(rename = "none")]
    NoneOf,
}

/// A test on one value: the condition forms the parser accepts, minus
/// the quantifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScalarTest {
    /// The value equals this one. A scalar in the file.
    Equals { value: ScalarValue },
    /// The value is one of these. A list in the file.
    OneOf { values: Vec<ScalarValue> },
    /// The value is none of these. The `not` operator.
    Not { values: Vec<ScalarValue> },
    /// The field has a value (`true`) or has none (`false`). The
    /// `is_set` operator.
    IsSet { value: bool },
}

/// One assertion operator in `require:`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct RequireRow {
    /// The field reference the assertion applies to.
    pub reference: String,
    pub test: RequireTest,
}

/// The assertion operators the parser accepts, one per row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequireTest {
    /// The field must be set.
    Required,
    /// The field must not be set.
    Forbidden,
    /// The value must be one of these.
    Values { values: Vec<ScalarValue> },
    /// The value must be none of these.
    Not { values: Vec<ScalarValue> },
    /// The value compares to another field's value (or `$today`).
    Compare {
        comparison: Comparison,
        field: String,
    },
    /// The related items number at least this many.
    MinCount { count: u32 },
    /// The related items number at most this many.
    MaxCount { count: u32 },
}

/// The field-to-field comparisons, named by their operator key minus
/// the `_field` suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// A literal in a condition or assertion: the JSON scalar the file's
/// YAML scalar reads as. A date is its ISO text; the parser coerces it
/// against the field's type on the way back in, as for a hand-written
/// file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(untagged)]
pub enum ScalarValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

// ── Parsed rule → rows ────────────────────────────────────────────────

impl RuleShape {
    /// The rows of a parsed rule, or why they cannot be built. The
    /// reasons are the two the module docs name; everything the parser
    /// accepts otherwise has a row.
    pub fn from_rule(rule: &Rule) -> Result<Self, String> {
        let mut match_rows = Vec::new();
        for (reference, condition) in &rule.match_conditions {
            match_rows.extend(match_rows_of(reference, condition)?);
        }
        let mut require_rows = Vec::new();
        for (reference, assertion) in &rule.require {
            require_rows.extend(require_rows_of(reference, assertion)?);
        }
        Ok(Self {
            match_rows,
            require_rows,
            count: rule.count.clone(),
        })
    }
}

fn match_rows_of(reference: &str, condition: &Condition) -> Result<Vec<MatchRow>, String> {
    let row = |quantifier: Option<Quantifier>, test: ScalarTest| MatchRow {
        reference: reference.to_owned(),
        quantifier,
        test,
    };
    let rows = match condition {
        Condition::Equals(value) => vec![row(
            None,
            ScalarTest::Equals {
                value: value.into(),
            },
        )],
        Condition::OneOf(values) => vec![row(
            None,
            ScalarTest::OneOf {
                values: scalar_values(values),
            },
        )],
        Condition::Operator(operator) => {
            let mut rows: Vec<MatchRow> = plain_tests(operator)
                .into_iter()
                .map(|test| row(None, test))
                .collect();
            let quantified = [
                (Quantifier::All, &operator.all),
                (Quantifier::Any, &operator.any),
                (Quantifier::NoneOf, &operator.none),
            ];
            for (quantifier, inner) in quantified {
                let Some(inner) = inner else {
                    continue;
                };
                for test in inner_tests(reference, inner)? {
                    rows.push(row(Some(quantifier), test));
                }
            }
            rows
        }
    };
    if rows.is_empty() {
        return Err(format!("the condition on '{reference}' is empty"));
    }
    Ok(rows)
}

/// The non-quantifier operators of a condition object, in a fixed
/// order; the parsed struct no longer knows the file's.
fn plain_tests(operator: &ConditionOperator) -> Vec<ScalarTest> {
    let ConditionOperator {
        not,
        is_set,
        all: _,
        any: _,
        none: _,
    } = operator;
    let mut tests = Vec::new();
    if let Some(negated) = not {
        tests.push(ScalarTest::Not {
            values: negation_values(negated),
        });
    }
    if let Some(set) = is_set {
        tests.push(ScalarTest::IsSet { value: *set });
    }
    tests
}

/// The tests inside a quantifier. A quantifier in here is the nesting
/// the evaluator ignores and the rows refuse.
fn inner_tests(reference: &str, condition: &Condition) -> Result<Vec<ScalarTest>, String> {
    match condition {
        Condition::Equals(value) => Ok(vec![ScalarTest::Equals {
            value: value.into(),
        }]),
        Condition::OneOf(values) => Ok(vec![ScalarTest::OneOf {
            values: scalar_values(values),
        }]),
        Condition::Operator(operator) => {
            if operator.all.is_some() || operator.any.is_some() || operator.none.is_some() {
                return Err(format!(
                    "'{reference}' nests a quantifier inside a quantifier, which is not evaluated"
                ));
            }
            let tests = plain_tests(operator);
            if tests.is_empty() {
                return Err(format!("the condition on '{reference}' is empty"));
            }
            Ok(tests)
        }
    }
}

fn require_rows_of(reference: &str, assertion: &Assertion) -> Result<Vec<RequireRow>, String> {
    let tests = match assertion {
        Assertion::Required => vec![RequireTest::Required],
        Assertion::Forbidden => vec![RequireTest::Forbidden],
        Assertion::Operator(operator) => assertion_tests(reference, operator)?,
    };
    if tests.is_empty() {
        return Err(format!("the assertion on '{reference}' is empty"));
    }
    Ok(tests
        .into_iter()
        .map(|test| RequireRow {
            reference: reference.to_owned(),
            test,
        })
        .collect())
}

/// The operators of an assertion object, in a fixed order.
fn assertion_tests(
    reference: &str,
    operator: &AssertionOperator,
) -> Result<Vec<RequireTest>, String> {
    let AssertionOperator {
        required,
        forbidden,
        values,
        not,
        eq_field,
        lt_field,
        lte_field,
        gt_field,
        gte_field,
        min_count,
        max_count,
    } = operator;
    let mut tests = Vec::new();
    for (flag, key, test) in [
        (required, "required", RequireTest::Required),
        (forbidden, "forbidden", RequireTest::Forbidden),
    ] {
        match flag {
            Some(true) => tests.push(test),
            Some(false) => {
                return Err(format!(
                    "'{reference}' sets '{key}: false', which has no meaning the editor can show"
                ))
            }
            None => {}
        }
    }
    if let Some(values) = values {
        tests.push(RequireTest::Values {
            values: scalar_values(values),
        });
    }
    if let Some(negated) = not {
        tests.push(RequireTest::Not {
            values: negation_values(negated),
        });
    }
    let comparisons = [
        (Comparison::Eq, eq_field),
        (Comparison::Lt, lt_field),
        (Comparison::Lte, lte_field),
        (Comparison::Gt, gt_field),
        (Comparison::Gte, gte_field),
    ];
    for (comparison, field) in comparisons {
        if let Some(field) = field {
            tests.push(RequireTest::Compare {
                comparison,
                field: field.clone(),
            });
        }
    }
    if let Some(count) = min_count {
        tests.push(RequireTest::MinCount { count: *count });
    }
    if let Some(count) = max_count {
        tests.push(RequireTest::MaxCount { count: *count });
    }
    Ok(tests)
}

impl From<&ConditionValue> for ScalarValue {
    fn from(value: &ConditionValue) -> Self {
        match value {
            ConditionValue::String(text) => Self::Text(text.clone()),
            ConditionValue::Number(number) => Self::Number(*number),
            ConditionValue::Bool(flag) => Self::Bool(*flag),
            ConditionValue::Date(date) => Self::Text(date.to_string()),
        }
    }
}

fn scalar_values(values: &[ConditionValue]) -> Vec<ScalarValue> {
    values.iter().map(ScalarValue::from).collect()
}

fn negation_values(negated: &NegationValue) -> Vec<ScalarValue> {
    match negated {
        NegationValue::Single(value) => vec![value.into()],
        NegationValue::Multiple(values) => scalar_values(values),
    }
}

// ── Rows → YAML tree ──────────────────────────────────────────────────

impl RuleShape {
    /// The `match`, `require` and `count` entries of the rule's mapping
    /// in the file, as `(key, value)` pairs, each present only when the
    /// shape has something for it; or why the rows cannot be written.
    ///
    /// Shorthand where the parser has one: a lone equality is a scalar,
    /// a lone membership a list, a lone `required`/`forbidden` the
    /// keyword, a lone one-value `values` the value itself (unless that
    /// value spells a keyword), a `not` of one value the value.
    pub fn to_yaml_entries(&self) -> Result<Vec<(&'static str, serde_yaml::Value)>, String> {
        let mut entries = Vec::new();
        if !self.match_rows.is_empty() {
            entries.push((
                "match",
                serde_yaml::Value::Mapping(match_mapping(&self.match_rows)?),
            ));
        }
        if !self.require_rows.is_empty() {
            entries.push((
                "require",
                serde_yaml::Value::Mapping(require_mapping(&self.require_rows)?),
            ));
        }
        if let Some(count) = &self.count {
            let mut mapping = serde_yaml::Mapping::new();
            if let Some(min) = count.min {
                mapping.insert(yaml_key("min"), min.into());
            }
            if let Some(max) = count.max {
                mapping.insert(yaml_key("max"), max.into());
            }
            entries.push(("count", serde_yaml::Value::Mapping(mapping)));
        }
        Ok(entries)
    }
}

fn match_mapping(rows: &[MatchRow]) -> Result<serde_yaml::Mapping, String> {
    let mut mapping = serde_yaml::Mapping::new();
    for reference in references_in_order(rows.iter().map(|row| row.reference.as_str())) {
        let rows_for: Vec<&MatchRow> = rows
            .iter()
            .filter(|row| row.reference == reference)
            .collect();
        let tests_under = |quantifier: Option<Quantifier>| -> Vec<&ScalarTest> {
            rows_for
                .iter()
                .filter(|row| row.quantifier == quantifier)
                .map(|row| &row.test)
                .collect()
        };
        let plain = tests_under(None);
        let quantified: Vec<(&'static str, Vec<&ScalarTest>)> = [
            ("all", Quantifier::All),
            ("any", Quantifier::Any),
            ("none", Quantifier::NoneOf),
        ]
        .into_iter()
        .map(|(key, quantifier)| (key, tests_under(Some(quantifier))))
        .filter(|(_, tests)| !tests.is_empty())
        .collect();

        let value = if quantified.is_empty() {
            condition_value(reference, &plain)?
        } else {
            let mut object = operator_mapping(reference, &plain)?;
            for (key, tests) in quantified {
                object.insert(yaml_key(key), condition_value(reference, &tests)?);
            }
            serde_yaml::Value::Mapping(object)
        };
        mapping.insert(yaml_key(reference), value);
    }
    Ok(mapping)
}

/// A condition from its tests: the scalar or list shorthand when one
/// test allows it, the operator object otherwise.
fn condition_value(reference: &str, tests: &[&ScalarTest]) -> Result<serde_yaml::Value, String> {
    match tests {
        [ScalarTest::Equals { value }] => Ok(scalar_yaml(value)),
        [ScalarTest::OneOf { values }] => Ok(list_yaml(values)),
        _ => Ok(serde_yaml::Value::Mapping(operator_mapping(
            reference, tests,
        )?)),
    }
}

/// The operator object of the tests that have an operator key. An
/// equality or membership has none — it is the shorthand's whole value —
/// so it cannot share a condition with another test.
fn operator_mapping(reference: &str, tests: &[&ScalarTest]) -> Result<serde_yaml::Mapping, String> {
    let mut mapping = serde_yaml::Mapping::new();
    for test in tests {
        let (key, value) = match test {
            ScalarTest::Equals { .. } | ScalarTest::OneOf { .. } => {
                return Err(format!(
                    "'{reference}': 'equals' and 'one of' cannot share a condition with another test; use 'not' or 'one of' alone"
                ))
            }
            ScalarTest::Not { values } => ("not", scalar_or_list_yaml(values)),
            ScalarTest::IsSet { value } => ("is_set", serde_yaml::Value::Bool(*value)),
        };
        insert_once(&mut mapping, reference, key, value)?;
    }
    Ok(mapping)
}

fn require_mapping(rows: &[RequireRow]) -> Result<serde_yaml::Mapping, String> {
    let mut mapping = serde_yaml::Mapping::new();
    for reference in references_in_order(rows.iter().map(|row| row.reference.as_str())) {
        let tests: Vec<&RequireTest> = rows
            .iter()
            .filter(|row| row.reference == reference)
            .map(|row| &row.test)
            .collect();
        mapping.insert(yaml_key(reference), assertion_value(reference, &tests)?);
    }
    Ok(mapping)
}

/// The words the parser reads as keywords where a scalar assertion
/// stands; a one-value `values` spelling one of them keeps its list.
const ASSERTION_KEYWORDS: [&str; 2] = ["required", "forbidden"];

fn assertion_value(reference: &str, tests: &[&RequireTest]) -> Result<serde_yaml::Value, String> {
    match tests {
        [RequireTest::Required] => return Ok(yaml_key("required")),
        [RequireTest::Forbidden] => return Ok(yaml_key("forbidden")),
        [RequireTest::Values { values }] if values.len() == 1 => {
            let spells_keyword = matches!(&values[0], ScalarValue::Text(text) if ASSERTION_KEYWORDS.contains(&text.as_str()));
            if !spells_keyword {
                return Ok(scalar_yaml(&values[0]));
            }
        }
        _ => {}
    }
    let mut mapping = serde_yaml::Mapping::new();
    for test in tests {
        let (key, value) = match test {
            RequireTest::Required => ("required", serde_yaml::Value::Bool(true)),
            RequireTest::Forbidden => ("forbidden", serde_yaml::Value::Bool(true)),
            RequireTest::Values { values } => ("values", list_yaml(values)),
            RequireTest::Not { values } => ("not", scalar_or_list_yaml(values)),
            RequireTest::Compare { comparison, field } => {
                (comparison.operator_key(), yaml_key(field))
            }
            RequireTest::MinCount { count } => ("min_count", (*count).into()),
            RequireTest::MaxCount { count } => ("max_count", (*count).into()),
        };
        insert_once(&mut mapping, reference, key, value)?;
    }
    Ok(serde_yaml::Value::Mapping(mapping))
}

impl Comparison {
    /// The assertion operator key this comparison is written as.
    pub fn operator_key(self) -> &'static str {
        match self {
            Self::Eq => "eq_field",
            Self::Lt => "lt_field",
            Self::Lte => "lte_field",
            Self::Gt => "gt_field",
            Self::Gte => "gte_field",
        }
    }
}

fn insert_once(
    mapping: &mut serde_yaml::Mapping,
    reference: &str,
    key: &str,
    value: serde_yaml::Value,
) -> Result<(), String> {
    if mapping.insert(yaml_key(key), value).is_some() {
        return Err(format!("'{reference}' has '{key}' twice"));
    }
    Ok(())
}

/// The distinct references in first-appearance order: the file keeps
/// the order the rows had.
fn references_in_order<'a>(references: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = Vec::new();
    for reference in references {
        if !seen.contains(&reference) {
            seen.push(reference);
        }
    }
    seen
}

fn yaml_key(key: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(key.to_owned())
}

/// A whole number is written as one: the parsed model holds every
/// number as a float, and `3.0` in the file would not read as the `3`
/// the author wrote.
fn scalar_yaml(value: &ScalarValue) -> serde_yaml::Value {
    match value {
        ScalarValue::Bool(flag) => serde_yaml::Value::Bool(*flag),
        ScalarValue::Number(number) => {
            let is_whole = number.fract() == 0.0 && number.abs() < 9_007_199_254_740_992.0;
            if is_whole {
                serde_yaml::Value::Number((*number as i64).into())
            } else {
                serde_yaml::Value::Number((*number).into())
            }
        }
        ScalarValue::Text(text) => serde_yaml::Value::String(text.clone()),
    }
}

fn list_yaml(values: &[ScalarValue]) -> serde_yaml::Value {
    serde_yaml::Value::Sequence(values.iter().map(scalar_yaml).collect())
}

fn scalar_or_list_yaml(values: &[ScalarValue]) -> serde_yaml::Value {
    match values {
        [single] => scalar_yaml(single),
        _ => list_yaml(values),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::schema::parse_schema;

    const FIELDS: &str = "\
fields:
  id:
    type: string
  status:
    type: choice
    values: [open, in_progress, done]
  priority:
    type: integer
  start_date:
    type: date
  end_date:
    type: date
  parent:
    type: link
    inverse: children
  depends_on:
    type: links
";

    /// Parse the schema `FIELDS` plus one rule, and return that rule.
    fn parse_rule(rule_yaml: &str) -> Rule {
        let yaml = format!("{FIELDS}rules:\n  - name: under-test\n{rule_yaml}");
        let schema = parse_schema(&yaml).unwrap_or_else(|error| panic!("{error}"));
        schema.rules.into_iter().next().unwrap()
    }

    fn rows_of(rule_yaml: &str) -> RuleShape {
        RuleShape::from_rule(&parse_rule(rule_yaml)).unwrap()
    }

    fn text(value: &str) -> ScalarValue {
        ScalarValue::Text(value.to_owned())
    }

    fn match_row(reference: &str, quantifier: Option<Quantifier>, test: ScalarTest) -> MatchRow {
        MatchRow {
            reference: reference.to_owned(),
            quantifier,
            test,
        }
    }

    fn require_row(reference: &str, test: RequireTest) -> RequireRow {
        RequireRow {
            reference: reference.to_owned(),
            test,
        }
    }

    /// The YAML text of a shape's entries, as the file would hold them.
    fn written(shape: &RuleShape) -> String {
        let mut mapping = serde_yaml::Mapping::new();
        for (key, value) in shape.to_yaml_entries().unwrap() {
            mapping.insert(yaml_key(key), value);
        }
        serde_yaml::to_string(&serde_yaml::Value::Mapping(mapping)).unwrap()
    }

    // ── Parsed rule → rows: every condition form ──

    #[test]
    fn scalar_and_list_conditions_become_one_row_each() {
        let shape = rows_of(
            "    match:\n      status: open\n      priority: [1, 2]\n    require:\n      id: required\n",
        );
        assert_eq!(
            shape.match_rows,
            vec![
                match_row(
                    "status",
                    None,
                    ScalarTest::Equals {
                        value: text("open")
                    }
                ),
                match_row(
                    "priority",
                    None,
                    ScalarTest::OneOf {
                        values: vec![ScalarValue::Number(1.0), ScalarValue::Number(2.0)]
                    }
                ),
            ]
        );
    }

    #[test]
    fn operator_object_becomes_one_row_per_operator() {
        let shape = rows_of(
            "    match:\n      status: { not: [open, done], is_set: true }\n    require:\n      id: required\n",
        );
        assert_eq!(
            shape.match_rows,
            vec![
                match_row(
                    "status",
                    None,
                    ScalarTest::Not {
                        values: vec![text("open"), text("done")]
                    }
                ),
                match_row("status", None, ScalarTest::IsSet { value: true }),
            ]
        );
    }

    #[test]
    fn quantifiers_become_rows_with_the_quantifier_set() {
        let shape = rows_of(
            "    match:\n      children.status:\n        any: in_progress\n        none: { not: done, is_set: true }\n    require:\n      id: required\n",
        );
        assert_eq!(
            shape.match_rows,
            vec![
                match_row(
                    "children.status",
                    Some(Quantifier::Any),
                    ScalarTest::Equals {
                        value: text("in_progress")
                    }
                ),
                match_row(
                    "children.status",
                    Some(Quantifier::NoneOf),
                    ScalarTest::Not {
                        values: vec![text("done")]
                    }
                ),
                match_row(
                    "children.status",
                    Some(Quantifier::NoneOf),
                    ScalarTest::IsSet { value: true }
                ),
            ]
        );
    }

    #[test]
    fn date_condition_is_served_as_its_iso_text() {
        let shape =
            rows_of("    match:\n      start_date: 2026-01-31\n    require:\n      id: required\n");
        assert_eq!(
            shape.match_rows[0].test,
            ScalarTest::Equals {
                value: text("2026-01-31")
            }
        );
    }

    #[test]
    fn nested_quantifier_is_refused_with_its_reference() {
        let rule = parse_rule(
            "    match:\n      children.status:\n        any: { all: done }\n    require:\n      id: required\n",
        );
        let reason = RuleShape::from_rule(&rule).unwrap_err();
        assert!(reason.contains("children.status"), "{reason}");
        assert!(reason.contains("nests a quantifier"), "{reason}");
    }

    // ── Parsed rule → rows: every assertion form ──

    #[test]
    fn assertion_keywords_and_scalar_shorthand_become_rows() {
        let shape = rows_of(
            "    require:\n      id: required\n      parent: forbidden\n      status: done\n",
        );
        assert_eq!(
            shape.require_rows,
            vec![
                require_row("id", RequireTest::Required),
                require_row("parent", RequireTest::Forbidden),
                require_row(
                    "status",
                    RequireTest::Values {
                        values: vec![text("done")]
                    }
                ),
            ]
        );
    }

    #[test]
    fn every_assertion_operator_becomes_a_row() {
        let shape = rows_of(
            "    require:\n      status: { required: true, values: [open, done], not: open }\n      end_date: { gte_field: start_date, lt_field: $today }\n      children: { min_count: 1, max_count: 5 }\n",
        );
        assert_eq!(
            shape.require_rows,
            vec![
                require_row("status", RequireTest::Required),
                require_row(
                    "status",
                    RequireTest::Values {
                        values: vec![text("open"), text("done")]
                    }
                ),
                require_row(
                    "status",
                    RequireTest::Not {
                        values: vec![text("open")]
                    }
                ),
                require_row(
                    "end_date",
                    RequireTest::Compare {
                        comparison: Comparison::Lt,
                        field: "$today".to_owned()
                    }
                ),
                require_row(
                    "end_date",
                    RequireTest::Compare {
                        comparison: Comparison::Gte,
                        field: "start_date".to_owned()
                    }
                ),
                require_row("children", RequireTest::MinCount { count: 1 }),
                require_row("children", RequireTest::MaxCount { count: 5 }),
            ]
        );
    }

    #[test]
    fn required_false_is_refused() {
        let rule = parse_rule("    require:\n      status: { required: false }\n");
        let reason = RuleShape::from_rule(&rule).unwrap_err();
        assert!(reason.contains("'required: false'"), "{reason}");
    }

    #[test]
    fn count_is_carried_as_is() {
        let shape = rows_of("    match:\n      status: open\n    count:\n      max: 3\n");
        assert_eq!(shape.count.as_ref().unwrap().max, Some(3));
        assert_eq!(shape.count.as_ref().unwrap().min, None);
    }

    // ── Rows → YAML: shorthand and merging ──

    #[test]
    fn lone_tests_are_written_in_shorthand() {
        let shape = RuleShape {
            match_rows: vec![
                match_row(
                    "status",
                    None,
                    ScalarTest::Equals {
                        value: text("open"),
                    },
                ),
                match_row(
                    "priority",
                    None,
                    ScalarTest::OneOf {
                        values: vec![ScalarValue::Number(1.0), ScalarValue::Number(2.5)],
                    },
                ),
                match_row(
                    "parent",
                    None,
                    ScalarTest::Not {
                        values: vec![text("root")],
                    },
                ),
            ],
            require_rows: vec![
                require_row("id", RequireTest::Required),
                require_row("parent", RequireTest::Forbidden),
                require_row(
                    "status",
                    RequireTest::Values {
                        values: vec![text("done")],
                    },
                ),
            ],
            count: None,
        };
        assert_eq!(
            written(&shape),
            "match:\n  status: open\n  priority:\n  - 1\n  - 2.5\n  parent:\n    not: root\nrequire:\n  id: required\n  parent: forbidden\n  status: done\n"
        );
    }

    #[test]
    fn rows_on_one_reference_merge_into_one_object() {
        let shape = RuleShape {
            match_rows: vec![
                match_row(
                    "children.status",
                    Some(Quantifier::Any),
                    ScalarTest::Equals {
                        value: text("on_hold"),
                    },
                ),
                match_row("children", None, ScalarTest::IsSet { value: true }),
                match_row(
                    "children.status",
                    Some(Quantifier::NoneOf),
                    ScalarTest::Equals {
                        value: text("in_progress"),
                    },
                ),
            ],
            require_rows: vec![
                require_row(
                    "status",
                    RequireTest::Values {
                        values: vec![text("in_progress"), text("on_hold")],
                    },
                ),
                require_row(
                    "status",
                    RequireTest::Not {
                        values: vec![text("done"), text("removed")],
                    },
                ),
            ],
            count: Some(CountConstraint {
                min: Some(1),
                max: None,
            }),
        };
        assert_eq!(
            written(&shape),
            "match:\n  children.status:\n    any: on_hold\n    none: in_progress\n  children:\n    is_set: true\nrequire:\n  status:\n    values:\n    - in_progress\n    - on_hold\n    not:\n    - done\n    - removed\ncount:\n  min: 1\n"
        );
    }

    #[test]
    fn a_value_spelling_a_keyword_keeps_its_list() {
        let shape = RuleShape {
            match_rows: Vec::new(),
            require_rows: vec![require_row(
                "status",
                RequireTest::Values {
                    values: vec![text("required")],
                },
            )],
            count: None,
        };
        assert_eq!(
            written(&shape),
            "require:\n  status:\n    values:\n    - required\n"
        );
    }

    #[test]
    fn equality_cannot_share_a_condition() {
        let shape = RuleShape {
            match_rows: vec![
                match_row(
                    "status",
                    None,
                    ScalarTest::Equals {
                        value: text("open"),
                    },
                ),
                match_row("status", None, ScalarTest::IsSet { value: true }),
            ],
            require_rows: Vec::new(),
            count: None,
        };
        let reason = shape.to_yaml_entries().unwrap_err();
        assert!(reason.contains("'status'"), "{reason}");
        assert!(reason.contains("cannot share"), "{reason}");
    }

    #[test]
    fn the_same_operator_twice_is_refused() {
        let shape = RuleShape {
            match_rows: Vec::new(),
            require_rows: vec![
                require_row(
                    "end_date",
                    RequireTest::Compare {
                        comparison: Comparison::Gte,
                        field: "start_date".to_owned(),
                    },
                ),
                require_row(
                    "end_date",
                    RequireTest::Compare {
                        comparison: Comparison::Gte,
                        field: "$today".to_owned(),
                    },
                ),
            ],
            count: None,
        };
        let reason = shape.to_yaml_entries().unwrap_err();
        assert_eq!(reason, "'end_date' has 'gte_field' twice");
    }

    #[test]
    fn written_rules_read_back_as_the_same_rows() {
        for rule_yaml in [
            "    match:\n      children.status:\n        all: [done, removed]\n        any: [done, removed]\n    require:\n      status:\n        values: [done]\n",
            "    match:\n      status: { not: open }\n      depends_on.status:\n        none: { is_set: false }\n    require:\n      end_date: { gte_field: start_date }\n      depends_on: { max_count: 2 }\n    count:\n      min: 1\n      max: 10\n",
        ] {
            let shape = rows_of(rule_yaml);
            let mut reindented = String::new();
            for line in written(&shape).lines() {
                reindented.push_str("    ");
                reindented.push_str(line);
                reindented.push('\n');
            }
            assert_eq!(rows_of(&reindented), shape, "{rule_yaml}");
        }
    }
}
