//! Cross-file validation of `compute:` configs — the schema-side
//! counterpart to [`crate::views_check`].
//!
//! The schema parser already guaranteed structure: every stored
//! [`ComputeConfig`](crate::model::schema::ComputeConfig) holds a
//! syntactically valid expression on a field whose declared type
//! participates in the algebra. What it could *not* do is resolve
//! references, because constants live in `resources.yaml` — so this
//! module, running once both files are loaded, checks that:
//!
//! - every referenced field and constant exists and has arithmetic,
//! - each expression's result type fits its field's declared type
//!   (with the one `integer → float` widening the algebra allows),
//! - compute references don't form a cycle.
//!
//! Findings are error-severity [`Diagnostic`]s pinned to `schema.yaml`,
//! not hard failures: a typo'd reference disables that computed field,
//! not the project. The evaluation pass skips any field reported here.

use std::collections::HashMap;
use std::path::Path;

use crate::expression::{check_types, ExpressionType, ReferenceResolution, TypeContext};
use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic};
use crate::model::resources::Resources;
use crate::model::schema::{FieldType, Schema, Severity};
use crate::model::FieldValue;

/// Check every `compute:` config against the schema and resources.
/// Returns one diagnostic per finding; an empty vector means every
/// computed field is safe to evaluate.
pub fn evaluate(schema: &Schema, resources: &Resources, schema_path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let context = ProjectTypeContext { schema, resources };

    for (field_name, field_definition) in &schema.fields {
        let Some(compute) = &field_definition.compute else {
            continue;
        };

        let result_type = match check_types(&compute.expression, &context) {
            Ok(result_type) => result_type,
            Err(error) => {
                diagnostics.push(compute_error(
                    schema_path,
                    ConfigDiagnosticKind::ComputeInvalidExpression {
                        field: field_name.clone(),
                        expression: compute.source.clone(),
                        detail: error.to_string(),
                    },
                ));
                continue;
            }
        };

        // The parser rejected compute on non-algebra declared types, so
        // this resolves for every field that got this far.
        let Some(declared_type) = expression_type_of(field_definition.field_type()) else {
            continue;
        };
        if !result_type.coerces_to(declared_type) {
            diagnostics.push(compute_error(
                schema_path,
                ConfigDiagnosticKind::ComputeResultTypeMismatch {
                    field: field_name.clone(),
                    expression: compute.source.clone(),
                    result_type: result_type.to_string(),
                    declared_type: declared_type.to_string(),
                },
            ));
        }
    }

    diagnostics.extend(detect_cycles(schema, schema_path));
    diagnostics
}

fn compute_error(schema_path: &Path, kind: ConfigDiagnosticKind) -> Diagnostic {
    Diagnostic::config(Severity::Error, schema_path.to_path_buf(), kind)
}

// ── Reference resolution ──────────────────────────────────────────────

/// [`TypeContext`] over the loaded schema and resources: field
/// references resolve through declared field types, constant references
/// through the coerced constant values.
struct ProjectTypeContext<'a> {
    schema: &'a Schema,
    resources: &'a Resources,
}

impl TypeContext for ProjectTypeContext<'_> {
    fn field(&self, name: &str) -> ReferenceResolution {
        match self.schema.fields.get(name) {
            None => ReferenceResolution::Unknown,
            Some(field_definition) => {
                let field_type = field_definition.field_type();
                match expression_type_of(field_type) {
                    Some(expression_type) => ReferenceResolution::Typed(expression_type),
                    None => ReferenceResolution::Unsupported {
                        type_name: field_type.to_string(),
                    },
                }
            }
        }
    }

    fn constant(&self, name: &str) -> ReferenceResolution {
        match self.resources.constant(name) {
            None => ReferenceResolution::Unknown,
            Some(FieldValue::Integer(_)) => ReferenceResolution::Typed(ExpressionType::Integer),
            Some(FieldValue::Float(_)) => ReferenceResolution::Typed(ExpressionType::Float),
            Some(FieldValue::Date(_)) => ReferenceResolution::Typed(ExpressionType::Date),
            Some(FieldValue::Duration(_)) => ReferenceResolution::Typed(ExpressionType::Duration),
            Some(other) => ReferenceResolution::Unsupported {
                type_name: value_type_name(other).to_owned(),
            },
        }
    }
}

/// The [`ExpressionType`] a declared field type participates as, or
/// `None` for types without arithmetic.
fn expression_type_of(field_type: FieldType) -> Option<ExpressionType> {
    match field_type {
        FieldType::Integer => Some(ExpressionType::Integer),
        FieldType::Float => Some(ExpressionType::Float),
        FieldType::Date => Some(ExpressionType::Date),
        FieldType::Duration => Some(ExpressionType::Duration),
        _ => None,
    }
}

/// Display name of a constant value's type, for the unsupported-type
/// message. Constants only ever hold the scalar subset.
fn value_type_name(value: &FieldValue) -> &'static str {
    match value {
        FieldValue::String(_) => "string",
        FieldValue::Boolean(_) => "boolean",
        FieldValue::Integer(_) => "integer",
        FieldValue::Float(_) => "float",
        FieldValue::Date(_) => "date",
        FieldValue::Duration(_) => "duration",
        _ => "unsupported",
    }
}

// ── Cycle detection ───────────────────────────────────────────────────

/// Colors for the depth-first search below.
#[derive(Clone, Copy, PartialEq)]
enum VisitState {
    InProgress,
    Done,
}

/// Find reference cycles among compute expressions. Only computed fields
/// have outgoing edges, so every cycle consists purely of computed
/// fields; each cycle is reported once, on the first field of it the
/// walk encounters (deterministic: schema declaration order).
fn detect_cycles(schema: &Schema, schema_path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut states: HashMap<&str, VisitState> = HashMap::new();

    for (field_name, field_definition) in &schema.fields {
        if field_definition.compute.is_some() && !states.contains_key(field_name.as_str()) {
            let mut stack = Vec::new();
            visit(schema, field_name, &mut states, &mut stack, &mut |chain| {
                diagnostics.push(compute_error(
                    schema_path,
                    ConfigDiagnosticKind::ComputeCycle { chain },
                ));
            });
        }
    }

    diagnostics
}

/// Depth-first walk from `field_name` along compute-reference edges.
fn visit<'a>(
    schema: &'a Schema,
    field_name: &'a str,
    states: &mut HashMap<&'a str, VisitState>,
    stack: &mut Vec<&'a str>,
    on_cycle: &mut impl FnMut(Vec<String>),
) {
    states.insert(field_name, VisitState::InProgress);
    stack.push(field_name);

    let compute = schema
        .fields
        .get(field_name)
        .and_then(|field_definition| field_definition.compute.as_ref());
    if let Some(compute) = compute {
        for referenced in compute.expression.field_references() {
            // Resolve to the schema's own key so the borrow outlives us.
            let Some((referenced, referenced_definition)) = schema.fields.get_key_value(referenced)
            else {
                continue; // unknown reference — reported by the type check
            };
            if referenced_definition.compute.is_none() {
                continue; // no outgoing edges, cannot be part of a cycle
            }
            match states.get(referenced.as_str()) {
                None => visit(schema, referenced, states, stack, on_cycle),
                Some(VisitState::InProgress) => {
                    let start = stack
                        .iter()
                        .position(|name| *name == referenced)
                        .expect("in-progress field is on the stack");
                    let mut chain: Vec<String> = stack[start..]
                        .iter()
                        .map(|name| (*name).to_owned())
                        .collect();
                    chain.push(referenced.to_owned());
                    on_cycle(chain);
                }
                Some(VisitState::Done) => {}
            }
        }
    }

    stack.pop();
    states.insert(field_name, VisitState::Done);
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::diagnostic::DiagnosticBody;
    use crate::parser::resources::parse_resources;
    use crate::parser::schema::parse_schema;

    fn check(schema_yaml: &str, resources_yaml: &str) -> Vec<Diagnostic> {
        let schema = parse_schema(schema_yaml).expect("test schema must parse");
        let resources = parse_resources(resources_yaml).expect("test resources must parse");
        evaluate(&schema, &resources, Path::new("schema.yaml"))
    }

    fn kinds(diagnostics: &[Diagnostic]) -> Vec<&ConfigDiagnosticKind> {
        diagnostics
            .iter()
            .map(|diagnostic| match &diagnostic.body {
                DiagnosticBody::Config(config) => &config.kind,
                other => panic!("expected config diagnostic, got {other:?}"),
            })
            .collect()
    }

    const SCHEDULING_FIELDS: &str = "\
fields:
  start_date:
    type: date
  duration:
    type: duration
  effort:
    type: duration
";

    #[test]
    fn valid_compute_produces_no_diagnostics() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  end_date:
    type: date
    compute: start_date + duration
  flow_efficiency:
    type: float
    compute: effort / duration
"
        );
        assert!(check(&schema_yaml, "").is_empty());
    }

    #[test]
    fn constant_reference_resolves_through_resources() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  cost:
    type: duration
    compute: effort * $constants.daily_rate
"
        );
        let resources_yaml = "\
constants:
  daily_rate:
    type: float
    value: 800
";
        assert!(check(&schema_yaml, resources_yaml).is_empty());
    }

    #[test]
    fn unknown_constant_is_reported() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  cost:
    type: duration
    compute: effort * $constants.daily_rate
"
        );
        let diagnostics = check(&schema_yaml, "");
        let kinds = kinds(&diagnostics);
        assert_eq!(kinds.len(), 1);
        assert!(matches!(
            kinds[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { field, detail, .. }
                if field == "cost" && detail.contains("unknown constant 'daily_rate'")
        ));
    }

    #[test]
    fn unknown_field_reference_is_reported() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  end_date:
    type: date
    compute: strat_date + duration
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("unknown field 'strat_date'")
        ));
    }

    #[test]
    fn reference_to_field_without_arithmetic_is_reported() {
        let schema_yaml = "\
fields:
  status:
    type: choice
    values: [open, done]
  weight:
    type: float
    compute: status * 2
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("choice")
        ));
    }

    #[test]
    fn algebra_violation_is_reported() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  end_date:
    type: date
    compute: start_date + start_date
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("cannot apply '+' to date and date")
        ));
    }

    #[test]
    fn result_type_mismatch_is_reported() {
        // date - date infers duration; declaring the field as date must fail.
        let schema_yaml = "\
fields:
  start_date:
    type: date
  end_date:
    type: date
  lead_time:
    type: date
    compute: end_date - start_date
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeResultTypeMismatch {
                field,
                result_type,
                declared_type,
                ..
            } if field == "lead_time" && result_type == "duration" && declared_type == "date"
        ));
    }

    #[test]
    fn integer_result_fits_float_field() {
        let schema_yaml = "\
fields:
  team_size:
    type: integer
  weight:
    type: float
    compute: team_size * 2
";
        assert!(check(schema_yaml, "").is_empty());
    }

    #[test]
    fn float_result_does_not_fit_integer_field() {
        let schema_yaml = "\
fields:
  team_size:
    type: integer
  doubled:
    type: integer
    compute: team_size * 2.0
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeResultTypeMismatch { .. }
        ));
    }

    #[test]
    fn string_constant_in_expression_is_reported() {
        let schema_yaml = "\
fields:
  weight:
    type: float
    compute: $constants.project_code * 2
";
        let resources_yaml = "\
constants:
  project_code:
    type: string
    value: WD
";
        let diagnostics = check(schema_yaml, resources_yaml);
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("string")
        ));
    }

    // ── Cycles ────────────────────────────────────────────────────────

    #[test]
    fn chain_without_cycle_is_clean() {
        // c depends on b depends on a — a chain, not a cycle.
        let schema_yaml = "\
fields:
  a:
    type: integer
  b:
    type: float
    compute: a * 2
  c:
    type: float
    compute: b * 2
";
        assert!(check(schema_yaml, "").is_empty());
    }

    #[test]
    fn three_field_cycle_is_reported_once_with_the_chain() {
        let schema_yaml = "\
fields:
  a:
    type: float
    compute: b * 1.0
  b:
    type: float
    compute: c * 1.0
  c:
    type: float
    compute: a * 1.0
";
        let diagnostics = check(schema_yaml, "");
        let cycle_chains: Vec<&Vec<String>> = kinds(&diagnostics)
            .into_iter()
            .filter_map(|kind| match kind {
                ConfigDiagnosticKind::ComputeCycle { chain } => Some(chain),
                _ => None,
            })
            .collect();
        assert_eq!(cycle_chains.len(), 1, "cycle must be reported exactly once");
        assert_eq!(cycle_chains[0], &vec!["a", "b", "c", "a"]);
    }

    #[test]
    fn self_reference_is_a_cycle() {
        let schema_yaml = "\
fields:
  a:
    type: float
    compute: a * 2
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics).last().unwrap(),
            ConfigDiagnosticKind::ComputeCycle { chain } if chain == &vec!["a", "a"]
        ));
    }

    #[test]
    fn reference_to_non_computed_field_creates_no_edge() {
        // end_date references duration; duration is aggregated but not
        // computed, so no edge and no cycle.
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  start_date:
    type: date
  duration:
    type: duration
    aggregate:
      function: sum
  end_date:
    type: date
    compute: start_date + duration
";
        assert!(check(schema_yaml, "").is_empty());
    }
}
