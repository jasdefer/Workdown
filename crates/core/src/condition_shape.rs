//! A `when:` branch condition as the field editor sees it: one
//! comparison as a row — left operand, operator, right operand — or,
//! when the expression is more than that, its text; and the row back as
//! the expression source the parser reads.
//!
//! The row is the case a person who does not know the syntax writes
//! ("status is done"), so it gets a form: an operand is a field, a
//! constant, `$today` or a literal entered through the value editor,
//! which removes the quoting trap (`status == done` is an unknown-field
//! error, `status == "done"` is the comparison). Arithmetic inside an
//! operand, a negation, or anything that is not one comparison is shown
//! and edited as text, the filter editor's existing pattern
//! (`schema-derived-field-editor`, decisions 1 and 2).
//!
//! Which it is, Rust decides, with an exhaustive match on the parsed
//! [`Expression`]: a node kind added to the grammar fails to compile
//! here until the row says whether it can hold it. There is no second
//! copy of the grammar anywhere.
//!
//! Every type here carries a `ts_rs` derive; `cargo xtask gen-types`
//! emits the TypeScript. A new type needs its `exports.add` line in
//! `crates/core/examples/gen_types.rs`.

use serde::{Deserialize, Serialize};

use crate::expression::{ComparisonOperator, Expression};

// ── Wire types ────────────────────────────────────────────────────────

/// A branch condition as the editor shows and sends it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConditionShape {
    /// One comparison: what the row editor edits.
    Row {
        left: Operand,
        operator: ComparisonOperator,
        right: Operand,
    },
    /// Anything else, as written; edited as text.
    Text { source: String },
}

/// One side of a row's comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operand {
    /// Another field of the same item.
    Field { name: String },
    /// A project constant, `$constants.<name>`.
    Constant { name: String },
    /// The evaluation date, `$today`.
    Today,
    /// A literal, entered through the value editor.
    Literal { value: ConditionLiteral },
}

/// A literal the grammar has: a whole number, a fraction, quoted text
/// or a boolean. No date — a date is compared against `$today` or a
/// field. Untagged on the wire, so it reads as the bare JSON scalar the
/// value editor hands over; a JSON number without a fraction is whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(untagged)]
pub enum ConditionLiteral {
    Integer(i64),
    Float(f64),
    Text(String),
    Boolean(bool),
}

// ── Expression ⇄ shape ────────────────────────────────────────────────

impl ConditionShape {
    /// The shape of a parsed condition whose text is `source`: a row
    /// when it is one comparison between two plain operands, its text
    /// otherwise.
    pub fn of(expression: &Expression, source: &str) -> ConditionShape {
        let text = || ConditionShape::Text {
            source: source.to_owned(),
        };
        match expression {
            Expression::Comparison {
                operator,
                left,
                right,
                ..
            } => match (Operand::of(left), Operand::of(right)) {
                (Some(left), Some(right)) => ConditionShape::Row {
                    left,
                    operator: *operator,
                    right,
                },
                _ => text(),
            },
            Expression::FieldReference { .. }
            | Expression::ConstantReference { .. }
            | Expression::TodayReference { .. }
            | Expression::IntegerLiteral { .. }
            | Expression::FloatLiteral { .. }
            | Expression::StringLiteral { .. }
            | Expression::BooleanLiteral { .. }
            | Expression::Negate { .. }
            | Expression::Binary { .. } => text(),
        }
    }

    /// The condition as the expression source the parser reads: a row
    /// spelled `left operator right`, text as it is.
    pub fn to_source(&self) -> String {
        match self {
            ConditionShape::Row {
                left,
                operator,
                right,
            } => format!("{} {operator} {}", left.to_source(), right.to_source()),
            ConditionShape::Text { source } => source.clone(),
        }
    }
}

impl Operand {
    /// The operand a plain node is; `None` for a node the row cannot
    /// hold — arithmetic, a negation, a nested comparison.
    fn of(expression: &Expression) -> Option<Operand> {
        match expression {
            Expression::FieldReference { name, .. } => Some(Operand::Field { name: name.clone() }),
            Expression::ConstantReference { name, .. } => {
                Some(Operand::Constant { name: name.clone() })
            }
            Expression::TodayReference { .. } => Some(Operand::Today),
            Expression::IntegerLiteral { value, .. } => Some(Operand::Literal {
                value: ConditionLiteral::Integer(*value),
            }),
            Expression::FloatLiteral { value, .. } => Some(Operand::Literal {
                value: ConditionLiteral::Float(*value),
            }),
            Expression::StringLiteral { value, .. } => Some(Operand::Literal {
                value: ConditionLiteral::Text(value.clone()),
            }),
            Expression::BooleanLiteral { value, .. } => Some(Operand::Literal {
                value: ConditionLiteral::Boolean(*value),
            }),
            Expression::Negate { .. }
            | Expression::Binary { .. }
            | Expression::Comparison { .. } => None,
        }
    }

    /// The operand as the grammar spells it. A fraction keeps its dot
    /// (`2.0`, not `2`), so it parses back as the fraction it was;
    /// text is quoted as the lexer reads it, which has no escapes.
    fn to_source(&self) -> String {
        match self {
            Operand::Field { name } => name.clone(),
            Operand::Constant { name } => format!("$constants.{name}"),
            Operand::Today => "$today".to_owned(),
            Operand::Literal { value } => match value {
                ConditionLiteral::Integer(integer) => integer.to_string(),
                ConditionLiteral::Float(float) => format!("{float:?}"),
                ConditionLiteral::Text(text) => format!("\"{text}\""),
                ConditionLiteral::Boolean(boolean) => boolean.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::parse_expression;

    fn shape(source: &str) -> ConditionShape {
        ConditionShape::of(&parse_expression(source).unwrap(), source)
    }

    #[test]
    fn one_comparison_between_plain_operands_is_a_row_that_spells_itself_back() {
        for source in [
            "status == \"done\"",
            "end_date < $today",
            "points >= $constants.limit",
            "flag != true",
            "ratio > 1.5",
            "$today <= deadline",
        ] {
            let row = shape(source);
            assert!(
                matches!(row, ConditionShape::Row { .. }),
                "{source} should be a row"
            );
            assert_eq!(row.to_source(), source);
        }
        assert_eq!(
            shape("status == \"done\""),
            ConditionShape::Row {
                left: Operand::Field {
                    name: "status".to_owned()
                },
                operator: ComparisonOperator::Equal,
                right: Operand::Literal {
                    value: ConditionLiteral::Text("done".to_owned())
                },
            }
        );
    }

    #[test]
    fn anything_else_is_its_text() {
        for source in [
            "points + 1 > 2",
            "-points < 0",
            "points",
            "points < 1 + 1",
            "flag",
        ] {
            assert_eq!(
                shape(source),
                ConditionShape::Text {
                    source: source.to_owned()
                },
                "{source} should be text"
            );
            assert_eq!(shape(source).to_source(), source);
        }
    }

    #[test]
    fn a_whole_fraction_keeps_its_dot_and_a_row_reads_back_as_the_same_row() {
        let row = shape("ratio == 2.0");
        assert_eq!(row.to_source(), "ratio == 2.0");
        let reparsed = parse_expression(&row.to_source()).unwrap();
        assert_eq!(ConditionShape::of(&reparsed, &row.to_source()), row);
    }

    #[test]
    fn the_wire_spells_the_tag_and_the_bare_literal() {
        let row = shape("points >= 3");
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["kind"], "row");
        assert_eq!(json["operator"], "greater_or_equal");
        assert_eq!(json["right"]["kind"], "literal");
        assert_eq!(json["right"]["value"], 3);
        let back: ConditionShape = serde_json::from_value(json).unwrap();
        assert_eq!(back, row);
    }
}
