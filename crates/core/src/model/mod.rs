//! Core data types: work items, schema definitions, and project configuration.

pub mod assertion;
pub mod calendar;
pub mod condition;
pub mod config;
pub mod diagnostic;
pub mod duration;
pub mod field_value;
pub mod rule;
pub mod schema;
pub mod template;
pub mod views;
pub mod weekday;
pub mod work_item;

pub use field_value::FieldValue;
pub use work_item::{WorkItem, WorkItemId};
