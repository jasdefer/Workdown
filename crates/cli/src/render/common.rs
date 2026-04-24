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
