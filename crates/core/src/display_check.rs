//! Shared validation of display-role field references.
//!
//! The display roles carry the same rules wherever they appear — a
//! view's `display:` block in `views.yaml` and the project-wide
//! `defaults.display` in `config.yaml`: the text roles (`title`,
//! `subtitle`, `fields`) are existence-only since any field's value
//! renders as text, while `color` must name a `color`-typed field (its
//! value feeds a background tint). The `none` sentinel of the color
//! role is always valid and checks nothing.
//!
//! This module owns those rules once. [`crate::views_check`] and
//! [`crate::config_check`] both call [`check_display_roles`] and map
//! the returned violations into their own diagnostic variants
//! (view-scoped there, config-scoped here), so the rules cannot drift
//! between the two files.

use crate::model::schema::{FieldType, Schema};
use crate::model::views::{ColorRole, DisplayConfig};

/// One of the four display roles, named so each checker can render the
/// slot path in its own scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayRole {
    Title,
    Subtitle,
    Fields,
    Color,
}

impl DisplayRole {
    /// The slot path as a per-view diagnostic reports it.
    pub(crate) fn view_slot(self) -> &'static str {
        match self {
            Self::Title => "display.title",
            Self::Subtitle => "display.subtitle",
            Self::Fields => "display.fields",
            Self::Color => "display.color",
        }
    }

    /// The slot path as a config-defaults diagnostic reports it.
    pub(crate) fn config_slot(self) -> &'static str {
        match self {
            Self::Title => "defaults.display.title",
            Self::Subtitle => "defaults.display.subtitle",
            Self::Fields => "defaults.display.fields",
            Self::Color => "defaults.display.color",
        }
    }
}

/// A role reference that fails validation. Scope-free — the caller
/// wraps it into its view- or config-scoped diagnostic.
#[derive(Debug)]
pub(crate) enum RoleViolation {
    /// The role names a field that resolves neither in `schema.fields`
    /// nor to the virtual `id`.
    UnknownField {
        role: DisplayRole,
        field_name: String,
    },
    /// The role names an existing field of an incompatible type. Only
    /// `color` is type-restricted today.
    TypeMismatch {
        role: DisplayRole,
        field_name: String,
        actual_type: FieldType,
        expected: &'static str,
    },
}

/// Check every role reference in one [`DisplayConfig`] against the
/// schema. Returns one violation per problem found; does not stop at
/// the first.
pub(crate) fn check_display_roles(display: &DisplayConfig, schema: &Schema) -> Vec<RoleViolation> {
    let mut violations = Vec::new();

    if let Some(field_name) = display.title.as_deref() {
        check_text_reference(DisplayRole::Title, field_name, schema, &mut violations);
    }
    if let Some(field_name) = display.subtitle.as_deref() {
        check_text_reference(DisplayRole::Subtitle, field_name, schema, &mut violations);
    }
    for field_name in display.fields.iter().flatten() {
        check_text_reference(DisplayRole::Fields, field_name, schema, &mut violations);
    }
    if let Some(ColorRole::Field(field_name)) = &display.color {
        check_color_reference(field_name, schema, &mut violations);
    }

    violations
}

/// Existence-only check for a text role: the virtual `id` and every
/// schema field are acceptable — any value renders as text.
fn check_text_reference(
    role: DisplayRole,
    field_name: &str,
    schema: &Schema,
    out: &mut Vec<RoleViolation>,
) {
    if field_name == "id" || schema.fields.contains_key(field_name) {
        return;
    }
    out.push(RoleViolation::UnknownField {
        role,
        field_name: field_name.to_owned(),
    });
}

/// The color role must name a `color`-typed schema field. The virtual
/// `id` exists everywhere but renders as a string — it can never feed a
/// tint, so it fails the type check like any other non-color field
/// instead of being silently accepted (and dead at extraction).
fn check_color_reference(field_name: &str, schema: &Schema, out: &mut Vec<RoleViolation>) {
    if field_name == "id" {
        out.push(RoleViolation::TypeMismatch {
            role: DisplayRole::Color,
            field_name: field_name.to_owned(),
            actual_type: FieldType::String,
            expected: "color",
        });
        return;
    }

    match schema.fields.get(field_name) {
        None => out.push(RoleViolation::UnknownField {
            role: DisplayRole::Color,
            field_name: field_name.to_owned(),
        }),
        Some(definition) if definition.field_type() != FieldType::Color => {
            out.push(RoleViolation::TypeMismatch {
                role: DisplayRole::Color,
                field_name: field_name.to_owned(),
                actual_type: definition.field_type(),
                expected: "color",
            });
        }
        Some(_) => {}
    }
}
