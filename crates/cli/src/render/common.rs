//! Shared helpers used across Markdown renderers.
//!
//! Kept deliberately small: only primitives that more than one renderer
//! needs. Renderer-specific formatting stays in its own module.

use workdown_core::view_data::Card;

/// Render a card as a Markdown link: `[title-or-id](base/id.md)`.
///
/// No bullet and no trailing newline — the caller decides indentation and
/// line structure. `item_link_base` is the path from the rendered view
/// file to the work items directory (e.g. `"../workdown-items"`).
pub fn card_link(card: &Card, item_link_base: &str) -> String {
    let link_text = card.title.as_deref().unwrap_or_else(|| card.id.as_str());
    let escaped = escape_link_text(link_text);
    format!("[{escaped}]({item_link_base}/{id}.md)", id = card.id)
}

/// Render a bare work item id as a Markdown link: `[id](base/id.md)`.
///
/// Used by renderers that have only an id and no `Card` to lean on (the
/// table renderer's `id` column, `Link`/`Links` cells). Workdown ids are
/// validated to `[a-z0-9][a-z0-9-]*`, so the link text needs no escaping.
pub fn id_link(id: &str, item_link_base: &str) -> String {
    format!("[{id}]({item_link_base}/{id}.md)")
}

/// Emit a one-line view description below the `# Heading`.
///
/// Renderers receive a description string from the dispatcher (built by
/// [`super::description::description_for`]). Empty strings — currently
/// only used for view kinds without a description — produce no output,
/// keeping the rendered file flush against its content.
pub fn emit_description(description: &str, out: &mut String) {
    if !description.is_empty() {
        out.push_str(description);
        out.push_str("\n\n");
    }
}

/// Escape characters that would break Markdown link-text parsing.
///
/// CommonMark terminates link text at unbalanced `]`, and a literal `\`
/// before a bracket needs its own escape to remain literal. Other
/// characters (parens, backticks, pipes, …) are fine inside link text.
pub fn escape_link_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' | '[' | ']' => {
                out.push('\\');
                out.push(character);
            }
            _ => out.push(character),
        }
    }
    out
}
