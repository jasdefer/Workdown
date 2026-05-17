//! Shared diagnostic-diff helper for mutation operations.
//!
//! Every mutation (`add`, `set`, `rename`, …) snapshots the project's
//! diagnostics before the write and again after. The exit code is driven
//! by whether the mutation *introduced* a new diagnostic — pre-existing
//! warnings elsewhere in the project remain visible but don't fail the
//! op. This module owns the diff so all callers agree on diagnostic
//! identity.

use std::collections::HashSet;

use crate::model::diagnostic::Diagnostic;

/// `true` iff any diagnostic exists in `post` that wasn't already in `pre`.
///
/// Identity is by stable JSON serialization — every `Diagnostic` field
/// is `Serialize`, and re-serializing the same data produces the same
/// string. Cheap because `pre` is hashed once.
pub(crate) fn introduced_by_mutation(pre: &[Diagnostic], post: &[Diagnostic]) -> bool {
    let pre_keys: HashSet<String> = pre.iter().filter_map(diagnostic_key).collect();
    post.iter().any(|diagnostic| {
        diagnostic_key(diagnostic)
            .map(|key| !pre_keys.contains(&key))
            .unwrap_or(true)
    })
}

fn diagnostic_key(diagnostic: &Diagnostic) -> Option<String> {
    serde_json::to_string(diagnostic).ok()
}
