//! The field values of one work item, each with where it came from.
//!
//! A value reaches an item in one of two ways: it was written in the
//! item's file, or a fill-in mechanism derived it at load. Both are
//! stored as one entry, so a value cannot exist without its origin and
//! an origin cannot exist without its value.
//!
//! Reading is origin-blind on purpose: [`ItemFields::get`] hands out
//! the value, and almost every consumer wants nothing else. Writing is
//! not: there is no plain `insert`, so every write states which of the
//! two ways it is.

use std::collections::HashMap;

use super::schema::FillMechanism;
use super::FieldValue;

/// Where a field's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueOrigin {
    /// Written in the item's file.
    Written,
    /// Derived at load by the named mechanism; not in the file.
    Derived(FillMechanism),
}

/// One field's value together with its origin.
#[derive(Debug, Clone, PartialEq)]
struct FieldEntry {
    value: FieldValue,
    origin: ValueOrigin,
}

/// Field names to their typed values, each carrying its origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemFields {
    entries: HashMap<String, FieldEntry>,
}

impl ItemFields {
    /// No fields.
    pub fn new() -> Self {
        Self::default()
    }

    /// The field's value, whatever its origin.
    pub fn get(&self, name: &str) -> Option<&FieldValue> {
        self.entries.get(name).map(|entry| &entry.value)
    }

    /// Whether the field has a value, whatever its origin.
    pub fn contains_key(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Where the field's value came from; `None` when it has no value.
    pub fn origin_of(&self, name: &str) -> Option<ValueOrigin> {
        self.entries.get(name).map(|entry| entry.origin)
    }

    /// Whether the field's value is written in the item's file.
    pub fn is_written(&self, name: &str) -> bool {
        self.origin_of(name) == Some(ValueOrigin::Written)
    }

    /// Set a value that is written in the item's file.
    pub fn insert_written(&mut self, name: String, value: FieldValue) {
        self.entries.insert(
            name,
            FieldEntry {
                value,
                origin: ValueOrigin::Written,
            },
        );
    }

    /// Set a value a fill-in mechanism derived.
    pub fn insert_derived(&mut self, name: String, value: FieldValue, mechanism: FillMechanism) {
        self.entries.insert(
            name,
            FieldEntry {
                value,
                origin: ValueOrigin::Derived(mechanism),
            },
        );
    }

    /// Every field name and value, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &FieldValue)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name, &entry.value))
    }

    /// Every field name, in no particular order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// The number of fields with a value.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no field has a value.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `fields["name"]` reads the value and panics when the field has
/// none, like indexing a map. For call sites that know it is there.
impl std::ops::Index<&str> for ItemFields {
    type Output = FieldValue;

    fn index(&self, name: &str) -> &FieldValue {
        &self.entries[name].value
    }
}

/// A plain map of values is what a file's frontmatter coerces to, so
/// every value in it is a written one.
impl From<HashMap<String, FieldValue>> for ItemFields {
    fn from(written: HashMap<String, FieldValue>) -> Self {
        written.into_iter().collect()
    }
}

/// Collecting name/value pairs builds written values, for the same
/// reason as the `From` above.
impl FromIterator<(String, FieldValue)> for ItemFields {
    fn from_iter<Pairs: IntoIterator<Item = (String, FieldValue)>>(pairs: Pairs) -> Self {
        let mut fields = Self::new();
        for (name, value) in pairs {
            fields.insert_written(name, value);
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_reads_the_same_whatever_its_origin() {
        let mut fields = ItemFields::new();
        fields.insert_written("effort".to_owned(), FieldValue::Integer(8));
        fields.insert_derived(
            "cost".to_owned(),
            FieldValue::Integer(640),
            FillMechanism::Compute,
        );

        assert_eq!(fields.get("effort"), Some(&FieldValue::Integer(8)));
        assert_eq!(fields.get("cost"), Some(&FieldValue::Integer(640)));
        assert_eq!(fields.origin_of("effort"), Some(ValueOrigin::Written));
        assert_eq!(
            fields.origin_of("cost"),
            Some(ValueOrigin::Derived(FillMechanism::Compute))
        );
    }

    #[test]
    fn a_field_without_a_value_has_no_origin() {
        let fields = ItemFields::new();

        assert_eq!(fields.get("effort"), None);
        assert_eq!(fields.origin_of("effort"), None);
        assert!(!fields.is_written("effort"));
    }

    #[test]
    fn collected_pairs_are_written_values() {
        let fields: ItemFields = [("effort".to_owned(), FieldValue::Integer(8))]
            .into_iter()
            .collect();

        assert!(fields.is_written("effort"));
    }
}
