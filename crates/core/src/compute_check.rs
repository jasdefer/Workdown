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
//! - every referenced field and constant exists and participates in the
//!   expression algebra (arithmetic types, or the equality-only text,
//!   boolean, and color types),
//! - each expression's result type fits its field's declared type
//!   (with the one `integer → float` widening the algebra allows),
//! - compute references don't form a cycle.
//!
//! Findings are error-severity [`Diagnostic`]s pinned to `schema.yaml`,
//! not hard failures: a typo'd reference disables that computed field,
//! not the project. The store's derive pass asks [`failed_fields`] for
//! the same findings and skips those fields, so a broken config never
//! adds per-item noise on top of its schema diagnostic.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::expression::{check_types, ExpressionType, ReferenceResolution, TypeContext};
use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic};
use crate::model::resources::Resources;
use crate::model::schema::{
    aggregate_result_type, allowed_aggregate_functions, FieldDefinition, FieldType,
    FieldTypeConfig, PullConfig, Schema, Severity,
};
use crate::model::FieldValue;

/// Check every `compute:` config against the schema and resources.
/// Returns one diagnostic per finding; an empty vector means every
/// computed field is safe to evaluate.
pub fn evaluate(schema: &Schema, resources: &Resources, schema_path: &Path) -> Vec<Diagnostic> {
    findings(schema, resources)
        .into_iter()
        .map(|finding| Diagnostic::config(Severity::Error, schema_path.to_path_buf(), finding.kind))
        .collect()
}

/// Names of the compute fields with a failing config — the set the
/// store's derive pass must skip. Same findings as [`evaluate`], as a
/// set instead of rendered diagnostics.
pub fn failed_fields(schema: &Schema, resources: &Resources) -> HashSet<String> {
    findings(schema, resources)
        .into_iter()
        .flat_map(|finding| finding.disabled_fields)
        .collect()
}

/// One check finding: the diagnostic kind plus the compute fields it
/// disables (a cycle disables every field on its chain).
struct Finding {
    disabled_fields: Vec<String>,
    kind: ConfigDiagnosticKind,
}

/// Run every check once, feeding both [`evaluate`] and [`failed_fields`].
fn findings(schema: &Schema, resources: &Resources) -> Vec<Finding> {
    let mut findings = Vec::new();
    let context = ProjectTypeContext { schema, resources };

    for (field_name, field_definition) in &schema.fields {
        let Some(compute) = &field_definition.compute else {
            continue;
        };

        let result_type = match check_types(&compute.expression, &context) {
            Ok(result_type) => result_type,
            Err(error) => {
                findings.push(Finding {
                    disabled_fields: vec![field_name.clone()],
                    kind: ConfigDiagnosticKind::ComputeInvalidExpression {
                        field: field_name.clone(),
                        expression: compute.source.clone(),
                        detail: error.to_string(),
                    },
                });
                continue;
            }
        };

        // The parser rejected compute on non-algebra declared types, so
        // this resolves for every field that got this far.
        let Some(declared_type) = expression_type_of(field_definition.field_type()) else {
            continue;
        };
        if !result_type.coerces_to(declared_type) {
            findings.push(Finding {
                disabled_fields: vec![field_name.clone()],
                kind: ConfigDiagnosticKind::ComputeResultTypeMismatch {
                    field: field_name.clone(),
                    expression: compute.source.clone(),
                    result_type: result_type.to_string(),
                    declared_type: declared_type.to_string(),
                },
            });
        }
    }

    // `when:` conditions: each branch must type-check as boolean, to
    // the same one-diagnostic-against-schema.yaml standard as compute.
    for (field_name, field_definition) in &schema.fields {
        let Some(when_config) = &field_definition.when else {
            continue;
        };
        for (index, branch) in when_config.branches.iter().enumerate() {
            let detail = match check_types(&branch.condition, &context) {
                Ok(ExpressionType::Boolean) => continue,
                Ok(other) => format!("condition has type {other}, expected boolean"),
                Err(error) => error.to_string(),
            };
            findings.push(Finding {
                disabled_fields: vec![field_name.clone()],
                kind: ConfigDiagnosticKind::WhenInvalidCondition {
                    field: field_name.clone(),
                    branch_number: index + 1,
                    condition: branch.condition_source.clone(),
                    detail,
                },
            });
        }
    }

    // `pull:` configs: `over` must be an acyclic link field, the
    // source field must exist, and the reduction must fit both ends.
    // Same standard as compute expressions: one finding against
    // `schema.yaml` disables the one field.
    for (field_name, field_definition) in &schema.fields {
        let Some(pull) = &field_definition.pull else {
            continue;
        };
        if let Some(detail) = pull_config_problem(schema, field_definition, pull) {
            findings.push(Finding {
                disabled_fields: vec![field_name.clone()],
                kind: ConfigDiagnosticKind::PullInvalidConfig {
                    field: field_name.clone(),
                    detail,
                },
            });
        }
    }

    findings.extend(cycle_findings(schema));
    findings
}

/// The first problem that makes a `pull:` config unevaluable, rendered
/// as the diagnostic detail — `None` when the config is sound.
fn pull_config_problem(
    schema: &Schema,
    field_definition: &FieldDefinition,
    pull: &PullConfig,
) -> Option<String> {
    let over = match schema.fields.get(&pull.over) {
        None => return Some(format!("'over' references unknown field '{}'", pull.over)),
        Some(over) => over,
    };
    let allow_cycles = match &over.type_config {
        FieldTypeConfig::Link { allow_cycles, .. }
        | FieldTypeConfig::Links { allow_cycles, .. } => *allow_cycles,
        _ => {
            return Some(format!(
                "'over' references field '{}' of type '{}' (must be 'link' or 'links')",
                pull.over,
                over.field_type()
            ))
        }
    };
    if allow_cycles != Some(false) {
        return Some(format!(
            "'over' field '{}' must declare allow_cycles: false — pulled values need an acyclic dependency graph to evaluate in",
            pull.over
        ));
    }

    let Some(source) = schema.fields.get(&pull.field) else {
        return Some(format!("'field' references unknown field '{}'", pull.field));
    };
    let source_type = source.field_type();
    let Some(allowed) = allowed_aggregate_functions(source_type) else {
        return Some(format!(
            "'field' references field '{}' of type '{source_type}', which no function can reduce",
            pull.field
        ));
    };
    if !allowed.contains(&pull.function) {
        let allowed_names: Vec<String> = allowed
            .iter()
            .map(|function| function.to_string())
            .collect();
        return Some(format!(
            "function '{}' is not valid for source field '{}' of type '{source_type}' (allowed: {})",
            pull.function,
            pull.field,
            allowed_names.join(", ")
        ));
    }
    let Some(result_type) = aggregate_result_type(pull.function, source_type) else {
        // Unreachable: the allowed-functions check above passed.
        return Some(format!(
            "function '{}' has no defined result for source type '{source_type}'",
            pull.function
        ));
    };

    let declared_type = field_definition.field_type();
    let fits = result_type == declared_type
        || (result_type == FieldType::Integer && declared_type == FieldType::Float);
    if !fits {
        return Some(format!(
            "{} of '{}' produces {result_type}, but the field is declared {declared_type}",
            pull.function, pull.field
        ));
    }
    None
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
            Some(FieldValue::Boolean(_)) => ReferenceResolution::Typed(ExpressionType::Boolean),
            Some(FieldValue::String(_)) => ReferenceResolution::Typed(ExpressionType::Text),
            Some(other) => ReferenceResolution::Unsupported {
                type_name: value_type_name(other).to_owned(),
            },
        }
    }
}

/// The [`ExpressionType`] a declared field type participates as, or
/// `None` for the collection types, which no expression operator
/// accepts.
fn expression_type_of(field_type: FieldType) -> Option<ExpressionType> {
    match field_type {
        FieldType::Integer => Some(ExpressionType::Integer),
        FieldType::Float => Some(ExpressionType::Float),
        FieldType::Date => Some(ExpressionType::Date),
        FieldType::Duration => Some(ExpressionType::Duration),
        FieldType::Boolean => Some(ExpressionType::Boolean),
        FieldType::String | FieldType::Choice => Some(ExpressionType::Text),
        FieldType::Color => Some(ExpressionType::Color),
        FieldType::Multichoice | FieldType::List | FieldType::Link | FieldType::Links => None,
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

/// Find reference cycles among derived fields (`compute:` expressions
/// and `when:` conditions share one dependency graph). Only derived
/// fields have outgoing edges, so every cycle consists purely of them;
/// each cycle is reported once, on the first field of it the walk
/// encounters (deterministic: schema declaration order).
fn cycle_findings(schema: &Schema) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut states: HashMap<&str, VisitState> = HashMap::new();

    for (field_name, field_definition) in &schema.fields {
        if field_definition.is_derived() && !states.contains_key(field_name.as_str()) {
            let mut stack = Vec::new();
            visit(schema, field_name, &mut states, &mut stack, &mut |chain| {
                findings.push(Finding {
                    // The chain repeats its first field at the end.
                    disabled_fields: chain[..chain.len() - 1].to_vec(),
                    kind: ConfigDiagnosticKind::ComputeCycle { chain },
                });
            });
        }
    }

    findings
}

/// Depth-first walk from `field_name` along derived-reference edges.
fn visit<'a>(
    schema: &'a Schema,
    field_name: &'a str,
    states: &mut HashMap<&'a str, VisitState>,
    stack: &mut Vec<&'a str>,
    on_cycle: &mut impl FnMut(Vec<String>),
) {
    states.insert(field_name, VisitState::InProgress);
    stack.push(field_name);

    if let Some(field_definition) = schema.fields.get(field_name) {
        for referenced in field_definition.derived_references() {
            // Resolve to the schema's own key so the borrow outlives us.
            let Some((referenced, referenced_definition)) = schema.fields.get_key_value(referenced)
            else {
                continue; // unknown reference — reported by the type check
            };
            if !referenced_definition.is_derived() {
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
    fn arithmetic_on_a_choice_field_is_reported() {
        // A choice reference types as text (usable in equality), so the
        // finding is the undefined *operation*, not the reference.
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
                if detail.contains("cannot apply '*' to text and integer")
        ));
    }

    #[test]
    fn reference_to_collection_field_is_reported() {
        let schema_yaml = "\
fields:
  tags:
    type: list
  weight:
    type: float
    compute: tags * 2
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("list")
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
        // The constant types as text; multiplying it is the error.
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeInvalidExpression { detail, .. }
                if detail.contains("cannot apply '*' to text and integer")
        ));
    }

    #[test]
    fn boolean_compute_with_predicates_is_clean() {
        let schema_yaml = "\
fields:
  status:
    type: choice
    values: [open, done]
  end_date:
    type: date
  is_overdue:
    type: boolean
    compute: end_date < $today
  is_done:
    type: boolean
    compute: status == \"done\"
";
        let diagnostics = check(schema_yaml, "");
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn boolean_result_on_a_non_boolean_field_is_a_type_mismatch() {
        let schema_yaml = "\
fields:
  end_date:
    type: date
  overdue_days:
    type: duration
    compute: end_date < $today
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeResultTypeMismatch { result_type, .. }
                if result_type == "boolean"
        ));
    }

    // ── when: conditions ───────────────────────────────────────────────

    #[test]
    fn boolean_when_conditions_are_clean() {
        let schema_yaml = "\
fields:
  status:
    type: choice
    values: [open, done]
  end_date:
    type: date
  urgency_color:
    type: color
    when:
      - if: status == \"done\"
        then: green
      - if: end_date > $today
        then: blue
    default: gray
";
        let diagnostics = check(schema_yaml, "");
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn non_boolean_when_condition_is_reported_with_branch_number() {
        let schema_yaml = "\
fields:
  start_date:
    type: date
  end_date:
    type: date
  tint:
    type: color
    when:
      - if: end_date > start_date
        then: green
      - if: end_date - start_date
        then: red
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::WhenInvalidCondition { field, branch_number, detail, .. }
                if field == "tint"
                    && *branch_number == 2
                    && detail.contains("has type duration, expected boolean")
        ));
    }

    #[test]
    fn unknown_reference_in_when_condition_is_reported() {
        let schema_yaml = "\
fields:
  tint:
    type: color
    when:
      - if: statsu == \"done\"
        then: green
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::WhenInvalidCondition { detail, .. }
                if detail.contains("unknown field 'statsu'")
        ));
        assert!(failed_fields(
            &crate::parser::schema::parse_schema(schema_yaml).unwrap(),
            &Resources::default()
        )
        .contains("tint"));
    }

    #[test]
    fn cycle_through_a_when_condition_is_reported() {
        // a computes from b; b's condition reads a — a loop across the
        // two derivation kinds, caught by the shared graph.
        let schema_yaml = "\
fields:
  a:
    type: boolean
    compute: b == true
  b:
    type: boolean
    when:
      - if: a == true
        then: false
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::ComputeCycle { chain }
                if chain.first().map(String::as_str) == Some("a") && chain.len() == 3
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
      over: parent
  end_date:
    type: date
    compute: start_date + duration
";
        assert!(check(schema_yaml, "").is_empty());
    }

    // ── pull: configs ─────────────────────────────────────────────────

    const PULL_FIELDS: &str = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  end:
    type: date
";

    #[test]
    fn valid_pull_config_produces_no_diagnostics() {
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
"
        );
        assert!(check(&schema_yaml, "").is_empty());
    }

    #[test]
    fn pull_over_unknown_field_is_reported() {
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: depends_no
      field: end
      function: max
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { field, detail }
                if field == "start" && detail.contains("unknown field 'depends_no'")
        ));
    }

    #[test]
    fn pull_over_non_link_field_is_reported() {
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: end
      field: end
      function: max
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { detail, .. }
                if detail.contains("of type 'date' (must be 'link' or 'links')")
        ));
    }

    #[test]
    fn pull_over_link_without_allow_cycles_false_is_reported() {
        let schema_yaml = "\
fields:
  related_to:
    type: links
  end:
    type: date
  start:
    type: date
    pull:
      over: related_to
      field: end
      function: max
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { detail, .. }
                if detail.contains("must declare allow_cycles: false")
        ));
    }

    #[test]
    fn pull_source_unknown_field_is_reported() {
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: depends_on
      field: endd
      function: max
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { detail, .. }
                if detail.contains("unknown field 'endd'")
        ));
    }

    #[test]
    fn pull_function_not_valid_for_source_type_is_reported() {
        // sum of dates is undefined — the aggregate table says so.
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: sum
"
        );
        let diagnostics = check(&schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { detail, .. }
                if detail.contains("function 'sum' is not valid for source field 'end' of type 'date'")
        ));
    }

    #[test]
    fn pull_result_type_mismatch_is_reported() {
        // count reduces to integer; declaring the field as date must fail.
        let schema_yaml = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  weight:
    type: integer
  start:
    type: date
    pull:
      over: depends_on
      field: weight
      function: count
";
        let diagnostics = check(schema_yaml, "");
        assert!(matches!(
            kinds(&diagnostics)[0],
            ConfigDiagnosticKind::PullInvalidConfig { detail, .. }
                if detail.contains("count of 'weight' produces integer, but the field is declared date")
        ));
    }

    #[test]
    fn pull_integer_result_fits_float_field() {
        let schema_yaml = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  weight:
    type: integer
  dependency_count:
    type: float
    pull:
      over: depends_on
      field: weight
      function: count
";
        assert!(check(schema_yaml, "").is_empty());
    }

    #[test]
    fn pull_over_single_link_field_is_accepted() {
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  end:
    type: date
  start:
    type: date
    pull:
      over: parent
      field: end
      function: max
";
        assert!(check(schema_yaml, "").is_empty());
    }

    #[test]
    fn failed_fields_includes_broken_pull_fields() {
        let schema_yaml = format!(
            "{PULL_FIELDS}  start:
    type: date
    pull:
      over: depends_on
      field: endd
      function: max
"
        );
        assert_eq!(failed(&schema_yaml, ""), vec!["start"]);
    }

    // ── failed_fields ─────────────────────────────────────────────────

    fn failed(schema_yaml: &str, resources_yaml: &str) -> Vec<String> {
        let schema = parse_schema(schema_yaml).expect("test schema must parse");
        let resources = parse_resources(resources_yaml).expect("test resources must parse");
        let mut names: Vec<String> = failed_fields(&schema, &resources).into_iter().collect();
        names.sort();
        names
    }

    #[test]
    fn failed_fields_is_empty_for_a_valid_schema() {
        let schema_yaml = format!(
            "{SCHEDULING_FIELDS}  end_date:
    type: date
    compute: start_date + duration
"
        );
        assert!(failed(&schema_yaml, "").is_empty());
    }

    #[test]
    fn failed_fields_collects_broken_fields_and_whole_cycles() {
        let schema_yaml = "\
fields:
  broken:
    type: float
    compute: typo * 2
  healthy:
    type: float
    compute: 1 + 1
  a:
    type: float
    compute: b * 1.0
  b:
    type: float
    compute: a * 1.0
";
        assert_eq!(failed(schema_yaml, ""), vec!["a", "b", "broken"]);
    }
}
