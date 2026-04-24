//! Board renderer — turns [`BoardData`] into a Markdown document.
//!
//! Output shape: a top-level `# Board: <field>` heading, then one `##`
//! section per column in extractor order. Each card is a bullet with a
//! Markdown link to the item file; the synthetic "no value" column
//! appears last as `## No <field>`. Empty columns show `_(no cards)_`.

use workdown_core::view_data::{BoardColumn, BoardData, Card};

use crate::render::common::card_link;

/// Render a `BoardData` as a Markdown string.
///
/// `item_link_base` is the relative path from the rendered view file to the
/// work items directory — e.g. `"../workdown-items"` for a default project
/// with the view written to `views/<id>.md`. The render command computes
/// this from `config.yaml` and passes it in.
pub fn render_board(data: &BoardData, item_link_base: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Board: {}\n\n", data.field));

    let sections: Vec<String> = data
        .columns
        .iter()
        .map(|column| render_column(column, &data.field, item_link_base))
        .collect();
    out.push_str(&sections.join("\n"));
    out
}

fn render_column(column: &BoardColumn, field: &str, item_link_base: &str) -> String {
    let heading = match &column.value {
        Some(value) => value.clone(),
        None => format!("No {field}"),
    };
    let mut section = format!("## {heading}\n");
    if column.cards.is_empty() {
        section.push_str("_(no cards)_\n");
    } else {
        for card in &column.cards {
            section.push_str(&render_card(card, item_link_base));
        }
    }
    section
}

fn render_card(card: &Card, item_link_base: &str) -> String {
    format!("- {}\n", card_link(card, item_link_base))
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::WorkItemId;
    use workdown_core::view_data::{BoardColumn, BoardData, Card};

    fn card(id: &str, title: Option<&str>) -> Card {
        Card {
            id: WorkItemId::from(id.to_owned()),
            title: title.map(str::to_owned),
            fields: vec![],
            body: String::new(),
        }
    }

    fn board(field: &str, columns: Vec<BoardColumn>) -> BoardData {
        BoardData {
            field: field.to_owned(),
            columns,
        }
    }

    fn column(value: Option<&str>, cards: Vec<Card>) -> BoardColumn {
        BoardColumn {
            value: value.map(str::to_owned),
            cards,
        }
    }

    #[test]
    fn renders_top_heading_with_field_name() {
        let data = board("status", vec![column(None, vec![])]);
        let output = render_board(&data, "../workdown-items");
        assert!(output.starts_with("# Board: status\n"));
    }

    #[test]
    fn renders_columns_in_extractor_order() {
        let data = board(
            "status",
            vec![
                column(Some("open"), vec![]),
                column(Some("in_progress"), vec![]),
                column(Some("done"), vec![]),
                column(None, vec![]),
            ],
        );
        let output = render_board(&data, "../workdown-items");
        let open_at = output.find("## open").expect("open heading");
        let in_progress_at = output.find("## in_progress").expect("in_progress heading");
        let done_at = output.find("## done").expect("done heading");
        let synthetic_at = output.find("## No status").expect("synthetic heading");
        assert!(open_at < in_progress_at);
        assert!(in_progress_at < done_at);
        assert!(done_at < synthetic_at);
    }

    #[test]
    fn synthetic_column_labeled_with_no_field_name() {
        let data = board("team", vec![column(None, vec![card("orphan", None)])]);
        let output = render_board(&data, "../workdown-items");
        assert!(output.contains("## No team\n"));
        assert!(!output.contains("## No status"));
    }

    #[test]
    fn empty_column_shows_no_cards_marker() {
        let data = board("status", vec![column(Some("done"), vec![])]);
        let output = render_board(&data, "../workdown-items");
        assert!(output.contains("## done\n_(no cards)_\n"));
    }

    #[test]
    fn card_with_title_links_title_text() {
        let data = board(
            "status",
            vec![column(
                Some("open"),
                vec![card("impl-login", Some("Implement user login"))],
            )],
        );
        let output = render_board(&data, "../workdown-items");
        assert!(output.contains("- [Implement user login](../workdown-items/impl-login.md)\n"));
    }

    #[test]
    fn card_without_title_links_id() {
        let data = board(
            "status",
            vec![column(Some("open"), vec![card("impl-login", None)])],
        );
        let output = render_board(&data, "../workdown-items");
        assert!(output.contains("- [impl-login](../workdown-items/impl-login.md)\n"));
    }

    #[test]
    fn link_text_escapes_brackets_and_backslashes() {
        let data = board(
            "status",
            vec![column(
                Some("open"),
                vec![card("weird", Some(r"has [brackets] and \ backslash"))],
            )],
        );
        let output = render_board(&data, "../workdown-items");
        assert!(
            output.contains(r"- [has \[brackets\] and \\ backslash](../workdown-items/weird.md)")
        );
    }

    #[test]
    fn uses_configured_item_link_base() {
        let data = board(
            "status",
            vec![column(Some("open"), vec![card("foo", None)])],
        );
        let output = render_board(&data, "../nested/items");
        assert!(output.contains("- [foo](../nested/items/foo.md)\n"));
    }

    #[test]
    fn blank_line_between_sections() {
        let data = board(
            "status",
            vec![
                column(Some("open"), vec![card("a", None)]),
                column(Some("done"), vec![card("b", None)]),
            ],
        );
        let output = render_board(&data, "../workdown-items");
        assert!(output.contains("](../workdown-items/a.md)\n\n## done\n"));
    }
}
