//! Generator token resolution for schema defaults and template values.
//!
//! A [`Generator`] token like `$today` or `$uuid` produces a concrete value
//! at `workdown add` time. Schema defaults use these (field-level), and
//! templates embed them inside frontmatter values. Both call through
//! [`resolve_generator`] so semantics stay identical.

use std::collections::HashMap;

use crate::model::schema::{DefaultValue, Generator};
use crate::model::FieldValue;
use crate::store::Store;

// ── Public API ───────────────────────────────────────────────────────

/// Resolve a schema [`DefaultValue`] into a concrete YAML value.
pub(crate) fn resolve_default(
    default: &DefaultValue,
    slug: &str,
    store: &Store,
    field_name: &str,
) -> serde_yaml::Value {
    match default {
        DefaultValue::String(string) => serde_yaml::Value::String(string.clone()),
        DefaultValue::Integer(number) => {
            serde_yaml::Value::Number(serde_yaml::Number::from(*number))
        }
        DefaultValue::Float(number) => {
            serde_yaml::to_value(number).unwrap_or(serde_yaml::Value::Null)
        }
        DefaultValue::Bool(flag) => serde_yaml::Value::Bool(*flag),
        DefaultValue::Generator(generator) => resolve_generator(generator, slug, store, field_name),
    }
}

/// Resolve a single [`Generator`] token into a concrete YAML value.
pub(crate) fn resolve_generator(
    generator: &Generator,
    slug: &str,
    store: &Store,
    field_name: &str,
) -> serde_yaml::Value {
    match generator {
        Generator::Filename => serde_yaml::Value::String(slug.to_owned()),
        Generator::FilenamePretty => serde_yaml::Value::String(prettify_slug(slug)),
        Generator::Uuid => serde_yaml::Value::String(uuid::Uuid::new_v4().to_string()),
        Generator::Today => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            serde_yaml::Value::String(today)
        }
        Generator::MaxPlusOne => {
            let max_value = resolve_max_plus_one(store, field_name);
            serde_yaml::Value::Number(serde_yaml::Number::from(max_value))
        }
    }
}

/// Walk template frontmatter and replace generator-token string values.
///
/// Exact-match only. A string value that exactly equals a known token (e.g.
/// `"$today"`) is replaced with the resolved value. Non-matching strings
/// pass through. Sequences of strings are walked element-by-element under
/// the same rule. Numbers, bools, nulls, and nested mappings pass through
/// unchanged.
///
/// Two-pass design: `slug_opt = None` resolves only slug-independent tokens
/// (`$today`, `$uuid`, `$max_plus_one`). `slug_opt = Some(&slug)` also
/// resolves `$filename` / `$filename_pretty`. Callers use the first pass to
/// resolve tokens inside the `id` field before slug derivation, and the
/// second pass for everything else.
pub(crate) fn resolve_template_tokens(
    frontmatter: &mut HashMap<String, serde_yaml::Value>,
    slug_opt: Option<&str>,
    store: &Store,
) {
    for (field_name, value) in frontmatter.iter_mut() {
        match value {
            serde_yaml::Value::String(string_value) => {
                if let Some(replacement) = try_resolve_token(string_value, slug_opt, store, field_name) {
                    *value = replacement;
                }
            }
            serde_yaml::Value::Sequence(sequence) => {
                for element in sequence.iter_mut() {
                    if let serde_yaml::Value::String(string_value) = element {
                        if let Some(replacement) =
                            try_resolve_token(string_value, slug_opt, store, field_name)
                        {
                            *element = replacement;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Private helpers ──────────────────────────────────────────────────

/// Attempt to interpret `string_value` as a known generator token and
/// resolve it. Returns `None` if the string is not a known token, or if
/// it requires a slug that the caller did not provide.
fn try_resolve_token(
    string_value: &str,
    slug_opt: Option<&str>,
    store: &Store,
    field_name: &str,
) -> Option<serde_yaml::Value> {
    let generator = token_to_generator(string_value)?;

    // First-pass callers (pre-slug-derivation) skip slug-dependent tokens
    // so that `$filename` inside a non-`id` field isn't resolved with a
    // placeholder. Second-pass callers always supply a slug.
    let slug = match generator {
        Generator::Filename | Generator::FilenamePretty => slug_opt?,
        Generator::Uuid | Generator::Today | Generator::MaxPlusOne => slug_opt.unwrap_or(""),
    };

    Some(resolve_generator(&generator, slug, store, field_name))
}

fn token_to_generator(string_value: &str) -> Option<Generator> {
    match string_value {
        "$filename" => Some(Generator::Filename),
        "$filename_pretty" => Some(Generator::FilenamePretty),
        "$uuid" => Some(Generator::Uuid),
        "$today" => Some(Generator::Today),
        "$max_plus_one" => Some(Generator::MaxPlusOne),
        _ => None,
    }
}

/// Find the maximum integer value of a field across all items, then add 1.
/// Returns 1 if no items have an integer value for this field.
fn resolve_max_plus_one(store: &Store, field_name: &str) -> i64 {
    let mut max: Option<i64> = None;
    for item in store.all_items() {
        if let Some(FieldValue::Integer(value)) = item.fields.get(field_name) {
            max = Some(max.map_or(*value, |current_max: i64| current_max.max(*value)));
        }
    }
    max.unwrap_or(0) + 1
}

/// Convert a slug like `"my-cool-task"` into `"My Cool Task"`.
pub(crate) fn prettify_slug(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                None => String::new(),
                Some(first) => {
                    let mut capitalized = first.to_uppercase().to_string();
                    capitalized.extend(characters);
                    capitalized
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig, Schema};

    // ── prettify_slug ────────────────────────────────────────────────

    #[test]
    fn prettify_simple_slug() {
        assert_eq!(prettify_slug("my-cool-task"), "My Cool Task");
    }

    #[test]
    fn prettify_single_word() {
        assert_eq!(prettify_slug("task"), "Task");
    }

    #[test]
    fn prettify_with_digits() {
        assert_eq!(prettify_slug("task-42"), "Task 42");
    }

    // ── resolve_max_plus_one ─────────────────────────────────────────

    #[test]
    fn max_plus_one_empty_store() {
        let schema = minimal_schema();
        let store = empty_store(&schema);
        assert_eq!(resolve_max_plus_one(&store, "order"), 1);
    }

    // ── resolve_template_tokens ──────────────────────────────────────

    #[test]
    fn tokens_resolves_today_exact_match() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "created".to_owned(),
            serde_yaml::Value::String("$today".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        let resolved = frontmatter.get("created").unwrap().as_str().unwrap();
        // YYYY-MM-DD shape.
        assert_eq!(resolved.len(), 10);
        assert_eq!(&resolved[4..5], "-");
        assert_eq!(&resolved[7..8], "-");
    }

    #[test]
    fn tokens_resolves_uuid_exact_match() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "assignee".to_owned(),
            serde_yaml::Value::String("$uuid".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        let resolved = frontmatter.get("assignee").unwrap().as_str().unwrap();
        // UUIDs are 36 chars with 4 hyphens.
        assert_eq!(resolved.len(), 36);
        assert_eq!(resolved.matches('-').count(), 4);
    }

    #[test]
    fn tokens_near_miss_stays_literal() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "title".to_owned(),
            serde_yaml::Value::String("before $today".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        assert_eq!(
            frontmatter.get("title").unwrap().as_str().unwrap(),
            "before $today"
        );
    }

    #[test]
    fn tokens_plain_string_untouched() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "type".to_owned(),
            serde_yaml::Value::String("bug".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        assert_eq!(frontmatter.get("type").unwrap().as_str().unwrap(), "bug");
    }

    #[test]
    fn tokens_inside_list_resolved_per_element() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        let sequence = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("$uuid".to_owned()),
            serde_yaml::Value::String("static".to_owned()),
            serde_yaml::Value::String("$today".to_owned()),
        ]);
        frontmatter.insert("tags".to_owned(), sequence);

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        let resolved = frontmatter.get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].as_str().unwrap().len(), 36); // UUID
        assert_eq!(resolved[1].as_str().unwrap(), "static");
        assert_eq!(resolved[2].as_str().unwrap().len(), 10); // Date
    }

    #[test]
    fn tokens_non_string_list_element_untouched() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        let sequence = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::Number(serde_yaml::Number::from(42)),
            serde_yaml::Value::String("$today".to_owned()),
        ]);
        frontmatter.insert("mix".to_owned(), sequence);

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        let resolved = frontmatter.get("mix").unwrap().as_sequence().unwrap();
        assert_eq!(resolved[0].as_i64().unwrap(), 42);
        assert_eq!(resolved[1].as_str().unwrap().len(), 10);
    }

    #[test]
    fn tokens_number_and_bool_untouched() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "count".to_owned(),
            serde_yaml::Value::Number(serde_yaml::Number::from(7)),
        );
        frontmatter.insert("done".to_owned(), serde_yaml::Value::Bool(true));

        resolve_template_tokens(&mut frontmatter, Some("slug"), &store);

        assert_eq!(frontmatter.get("count").unwrap().as_i64().unwrap(), 7);
        assert_eq!(frontmatter.get("done").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn tokens_filename_requires_slug() {
        // Pre-slug pass: $filename should be skipped.
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "linked".to_owned(),
            serde_yaml::Value::String("$filename".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, None, &store);

        // Unchanged — needs slug.
        assert_eq!(
            frontmatter.get("linked").unwrap().as_str().unwrap(),
            "$filename"
        );

        // Second pass with slug resolves it.
        resolve_template_tokens(&mut frontmatter, Some("my-task"), &store);
        assert_eq!(
            frontmatter.get("linked").unwrap().as_str().unwrap(),
            "my-task"
        );
    }

    #[test]
    fn tokens_uuid_and_today_resolve_without_slug() {
        let store = empty_store(&minimal_schema());
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "id".to_owned(),
            serde_yaml::Value::String("$uuid".to_owned()),
        );

        resolve_template_tokens(&mut frontmatter, None, &store);

        let resolved = frontmatter.get("id").unwrap().as_str().unwrap();
        assert_eq!(resolved.len(), 36);
    }

    // ── test helpers ─────────────────────────────────────────────────

    fn minimal_schema() -> Schema {
        use indexmap::IndexMap;

        let mut fields = IndexMap::new();
        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn empty_store(schema: &Schema) -> Store {
        let directory = tempfile::tempdir().unwrap();
        Store::load(directory.path(), schema).unwrap()
    }
}
