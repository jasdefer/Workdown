//! Wire contract for the project identity endpoint (`GET /api/project`).
//!
//! The narrow projection of `config.yaml`'s `project:` block — the
//! project's name and description, nothing else. Deliberately not a
//! config endpoint: `paths.*` is local filesystem layout the browser has
//! no business knowing, and a "here is the config" shape would invite
//! shipping it. Anything else the web shell needs about the project
//! itself joins this contract field by field.
//!
//! Like the timer and git contracts, it lives in core so
//! `cargo xtask gen-types` can emit the TypeScript binding.

use serde::Serialize;

use crate::model::config::ProjectMeta;

/// Who the project is, as the browser sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct ProjectIdentity {
    /// `project.name` from `config.yaml` — free-form user text. The web
    /// app puts it in the browser tab title, so every consumer must
    /// treat it as text, never as markup.
    pub name: String,
    /// `project.description`, or `None` when the project left it blank.
    /// Empty and absent are the same thing to a reader, so the blank
    /// case is normalised here rather than in every client.
    pub description: Option<String>,
}

/// Project the config's `project:` block onto the wire shape.
pub fn build(project: &ProjectMeta) -> ProjectIdentity {
    let description = project.description.trim();
    ProjectIdentity {
        name: project.name.clone(),
        description: if description.is_empty() {
            None
        } else {
            Some(description.to_owned())
        },
    }
}
