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
/// The yes/no form of [`introduced_diagnostics`], for callers that only
/// drive an exit code or a flag from the answer.
pub(crate) fn introduced_by_mutation(pre: &[Diagnostic], post: &[Diagnostic]) -> bool {
    !introduced_diagnostics(pre, post).is_empty()
}

/// The diagnostics in `post` that weren't already in `pre`, in `post`'s
/// order — what a change *introduced*, as opposed to what the project
/// already reported.
///
/// Identity is by stable JSON serialization — every `Diagnostic` field
/// is `Serialize`, and re-serializing the same data produces the same
/// string. Cheap because `pre` is hashed once. A diagnostic that fails
/// to serialize (which none does) counts as new rather than as known.
pub(crate) fn introduced_diagnostics(pre: &[Diagnostic], post: &[Diagnostic]) -> Vec<Diagnostic> {
    let pre_keys: HashSet<String> = pre.iter().filter_map(diagnostic_key).collect();
    post.iter()
        .filter(|diagnostic| {
            diagnostic_key(diagnostic)
                .map(|key| !pre_keys.contains(&key))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

fn diagnostic_key(diagnostic: &Diagnostic) -> Option<String> {
    serde_json::to_string(diagnostic).ok()
}
