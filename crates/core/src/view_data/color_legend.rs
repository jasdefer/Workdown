//! What the colors of a view mean.
//!
//! A view tints its items with the field the `color` display role
//! picks. This module says what those tints stand for, read from the
//! field's recipe in the schema rather than guessed from the items:
//!
//! - **`when:`** is a table from conditions to literal colors, so it
//!   gets a full legend: one entry per color that is on screen, with
//!   the conditions that produce it. Two branches with the same `then`
//!   share an entry, and so does the evaluated `default`.
//! - **`pull:`** copies the color of a linked item. The schema knows
//!   where the color comes from and nothing about what it stands for,
//!   so the answer is a hint, not a legend.
//! - A field no mechanism fills has no stated meaning at all, and
//!   there is no legend.
//!
//! The recipe supplies the wording and the items supply the selection:
//! only colors of items the view presents appear, each with the number
//! of items wearing it. A hand-written color on a `when:` field is not
//! explained by any condition, so it is counted apart.
//!
//! No part of this needs to know which branch an item matched. A
//! `then:` is a literal, so a derived color names its entry by itself.
//! What that leaves open is an entry several branches share, or a
//! branch and the `default`: the recipe says what *can* make an item
//! gray, not what made the ones on screen gray. Where every condition
//! tests the same field for equality, the legend answers with the
//! values that field has on the counted items.
//!
//! Wording stays with the renderers (ADR-006): a condition travels as
//! the [`ConditionShape`] the schema editor already reads.

use std::collections::HashSet;

use serde::Serialize;

use crate::condition_shape::{ConditionShape, Operand};
use crate::expression::ComparisonOperator;
use crate::model::color::resolve_color_to_hex;
use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldTypeConfig, Recipe, Schema, WhenConfig};
use crate::model::views::View;
use crate::model::{FieldValue, WorkItem};
use crate::store::Store;

use super::common::resolve_color_field;
use super::tree::TreeNode;
use super::treemap::TreemapNode;
use super::ViewData;

// ── Wire types ────────────────────────────────────────────────────────

/// The meaning of the tints in one view.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ColorLegend {
    /// The `color`-typed field the view tints with.
    pub field: String,
    pub meaning: ColorMeaning,
}

/// How much the schema can say about a color field's values.
///
/// A `when:` field takes one of the first two forms. Which one is
/// decided once for the whole legend, so a reader never meets a row
/// that mixes values and conditions.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColorMeaning {
    /// Every condition on screen tests `subject` for equality with a
    /// literal: the legend reads "Color by Status" and each color is
    /// labelled with the values that field has on its items.
    ByValue {
        subject: String,
        /// One entry per derived color on screen, in the order the
        /// recipe first names the color.
        entries: Vec<ValueEntry>,
        /// Colors written by hand on items of the view, which no
        /// condition explains. In the view's item order.
        written: Vec<ColorCount>,
    },
    /// The conditions differ in field or form, so each color is
    /// labelled with the conditions that produce it.
    ByCondition {
        /// One entry per derived color on screen, in the order the
        /// recipe first names the color.
        entries: Vec<ConditionEntry>,
        /// As in [`ColorMeaning::ByValue`].
        written: Vec<ColorCount>,
    },
    /// A `pull:` field: the color is the one `source_field` has on the
    /// item `over` points at.
    Inherited { over: String, source_field: String },
}

/// One color of a `when:` field, labelled with what its items hold.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ValueEntry {
    /// Resolved `#rrggbb`, the same form as an item's `background`.
    pub color: String,
    /// Items of the view that derived this color.
    pub count: usize,
    /// The values the subject field has on the counted items, `None`
    /// for an item without one. This is what is on screen, not what
    /// the recipe allows, so a view filtered down to the items of the
    /// `default` names their values instead of the branch that shares
    /// the color. In the field's declared order for a `choice`, else
    /// as the view lists them; a missing value last.
    pub values: Vec<Option<String>>,
}

/// One color of a `when:` field, labelled with what makes an item
/// take it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ConditionEntry {
    /// Resolved `#rrggbb`, the same form as an item's `background`.
    pub color: String,
    /// Items of the view that derived this color.
    pub count: usize,
    /// The conditions of every branch whose `then` is this color, in
    /// branch order. Empty when only the `default` produces it.
    pub conditions: Vec<ConditionShape>,
    /// Whether the recipe's `default` is this color as well, so items
    /// that matched no branch are among the counted ones.
    pub otherwise: bool,
}

/// A color and the number of items of the view wearing it.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct ColorCount {
    /// Resolved `#rrggbb`.
    pub color: String,
    pub count: usize,
}

// ── Building ──────────────────────────────────────────────────────────

/// The legend for `view`'s tints over the items `data` presents, or
/// `None` when there is nothing to explain: tinting is off, the color
/// field has no recipe, or no presented item has a color.
///
/// `view` must be the view `data` was extracted from, display roles
/// resolved, so both agree on the color field.
pub fn color_legend(
    view: &View,
    data: &ViewData,
    store: &Store,
    schema: &Schema,
) -> Option<ColorLegend> {
    let field_name = resolve_color_field(schema, Some(&view.display))?;
    let definition = schema.fields.get(field_name)?;
    let recipe = definition.recipe()?;

    let tints = presented_tints(data, store, field_name);
    if tints.is_empty() {
        return None;
    }

    // Exhaustive, so a new recipe has to say what it can tell a reader
    // about its colors before it compiles.
    let meaning = match recipe {
        Recipe::When(when) => when_meaning(when, &tints, schema),
        Recipe::Pull(pull) => ColorMeaning::Inherited {
            over: pull.over.clone(),
            source_field: pull.field.clone(),
        },
        // The parser refuses `compute` on a color field.
        Recipe::Compute(_) => return None,
    };

    Some(ColorLegend {
        field: field_name.to_owned(),
        meaning,
    })
}

/// The tint of one presented item.
struct Tint<'store> {
    hex: String,
    /// Written in the item's file rather than derived.
    written: bool,
    item: &'store WorkItem,
}

/// The tint of every item `data` presents that has one, each item
/// once, in the order the view lists them.
fn presented_tints<'store>(
    data: &ViewData,
    store: &'store Store,
    field_name: &str,
) -> Vec<Tint<'store>> {
    let mut seen: HashSet<&str> = HashSet::new();
    tinted_item_ids(data)
        .into_iter()
        .filter(|id| seen.insert(*id))
        .filter_map(|id| {
            let item = store.get(id)?;
            Some(Tint {
                hex: hex_of(item.fields.get(field_name)?)?,
                written: item.fields.is_written(field_name),
                item,
            })
        })
        .collect()
}

/// The ids of the items a view kind draws with a tint, repeats
/// included. Unplaced items are left out: they are listed below the
/// view untinted. The chart kinds aggregate items and tint none.
///
/// Exhaustive over [`ViewData`]: a new kind has to say which of its
/// items wear the color.
fn tinted_item_ids(data: &ViewData) -> Vec<&str> {
    match data {
        ViewData::Board(board) => board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .map(|card| card.id.as_str())
            .collect(),
        ViewData::Gantt(gantt) => gantt.bars.iter().map(|bar| bar.card.id.as_str()).collect(),
        ViewData::GanttByDepth(gantt) => gantt
            .levels
            .iter()
            .flat_map(|level| &level.bars)
            .map(|bar| bar.card.id.as_str())
            .collect(),
        ViewData::GanttByInitiative(gantt) => gantt
            .initiatives
            .iter()
            .flat_map(|initiative| &initiative.bars)
            .map(|bar| bar.card.id.as_str())
            .collect(),
        ViewData::Graph(graph) => graph.nodes.iter().map(|card| card.id.as_str()).collect(),
        ViewData::Table(table) => table.rows.iter().map(|row| row.id.as_str()).collect(),
        ViewData::Tree(tree) => {
            let mut ids = Vec::new();
            for root in &tree.roots {
                collect_tree_ids(root, &mut ids);
            }
            ids
        }
        ViewData::Treemap(treemap) => {
            let mut ids = Vec::new();
            collect_treemap_ids(&treemap.root, &mut ids);
            ids
        }
        ViewData::BarChart(_)
        | ViewData::Heatmap(_)
        | ViewData::LineChart(_)
        | ViewData::Metric(_)
        | ViewData::Workload(_) => Vec::new(),
    }
}

fn collect_tree_ids<'data>(node: &'data TreeNode, ids: &mut Vec<&'data str>) {
    ids.push(node.id.as_str());
    for child in &node.children {
        collect_tree_ids(child, ids);
    }
}

fn collect_treemap_ids<'data>(node: &'data TreemapNode, ids: &mut Vec<&'data str>) {
    if let Some(card) = &node.card {
        ids.push(card.id.as_str());
    }
    for child in &node.children {
        collect_treemap_ids(child, ids);
    }
}

/// One color a `when:` recipe can produce: what produces it, and the
/// presented items that derived it.
struct RecipeColor<'store> {
    hex: String,
    /// The conditions of every branch whose `then` is this color.
    conditions: Vec<ConditionShape>,
    /// Whether the `default` is this color as well.
    otherwise: bool,
    items: Vec<&'store WorkItem>,
}

/// The legend of a `when:` field: the recipe's colors, narrowed to the
/// ones `tints` wear, and labelled by value where the conditions allow
/// it, else by condition.
fn when_meaning(when: &WhenConfig, tints: &[Tint<'_>], schema: &Schema) -> ColorMeaning {
    let mut colors = recipe_colors(when);
    let mut written: Vec<ColorCount> = Vec::new();
    for tint in tints {
        if tint.written {
            count_written(&mut written, &tint.hex);
        } else if let Some(color) = colors.iter_mut().find(|color| color.hex == tint.hex) {
            color.items.push(tint.item);
        }
    }
    colors.retain(|color| !color.items.is_empty());

    match shared_subject(&colors).map(str::to_owned) {
        Some(subject) => {
            let entries = colors
                .iter()
                .map(|color| ValueEntry {
                    color: color.hex.clone(),
                    count: color.items.len(),
                    values: values_on(&color.items, &subject, schema),
                })
                .collect();
            ColorMeaning::ByValue {
                subject,
                entries,
                written,
            }
        }
        None => {
            let entries = colors
                .into_iter()
                .map(|color| ConditionEntry {
                    count: color.items.len(),
                    color: color.hex,
                    conditions: color.conditions,
                    otherwise: color.otherwise,
                })
                .collect();
            ColorMeaning::ByCondition { entries, written }
        }
    }
}

/// Every color the recipe names, in the order it first names it, with
/// no items yet. Two branches with the same `then` share a color, and
/// so does the `default`.
fn recipe_colors<'store>(when: &WhenConfig) -> Vec<RecipeColor<'store>> {
    let mut colors: Vec<RecipeColor<'store>> = Vec::new();
    for branch in &when.branches {
        if let Some(hex) = hex_of(&branch.value) {
            let condition = ConditionShape::of(&branch.condition, &branch.condition_source);
            recipe_color(&mut colors, hex).conditions.push(condition);
        }
    }
    if let Some(hex) = when.default.as_ref().and_then(hex_of) {
        recipe_color(&mut colors, hex).otherwise = true;
    }
    colors
}

/// The entry of `hex`, appended empty when the recipe has not named
/// the color before.
fn recipe_color<'colors, 'store>(
    colors: &'colors mut Vec<RecipeColor<'store>>,
    hex: String,
) -> &'colors mut RecipeColor<'store> {
    let index = match colors.iter().position(|color| color.hex == hex) {
        Some(index) => index,
        None => {
            colors.push(RecipeColor {
                hex,
                conditions: Vec::new(),
                otherwise: false,
                items: Vec::new(),
            });
            colors.len() - 1
        }
    };
    &mut colors[index]
}

/// Count one more item wearing the hand-written `hex`.
fn count_written(written: &mut Vec<ColorCount>, hex: &str) {
    match written.iter_mut().find(|known| known.color == hex) {
        Some(known) => known.count += 1,
        None => written.push(ColorCount {
            color: hex.to_owned(),
            count: 1,
        }),
    }
}

/// The distinct values `subject` has on `items`: in the field's
/// declared order for a `choice`, else as the view lists them; a
/// missing value last.
fn values_on(items: &[&WorkItem], subject: &str, schema: &Schema) -> Vec<Option<String>> {
    let mut values: Vec<Option<String>> = Vec::new();
    for item in items {
        let value = item.fields.get(subject).map(format_field_value);
        if !values.contains(&value) {
            values.push(value);
        }
    }
    let declared = declared_values(schema, subject);
    // Stable, so values the field does not declare keep the view's
    // order among themselves.
    values.sort_by_key(|value| match value {
        Some(value) => declared
            .iter()
            .position(|known| known == value)
            .unwrap_or(declared.len()),
        None => usize::MAX,
    });
    values
}

/// The field every condition of `colors` tests for equality with a
/// literal, when there is a condition and they all test the same one.
fn shared_subject<'colors>(colors: &'colors [RecipeColor<'_>]) -> Option<&'colors str> {
    let mut subject: Option<&str> = None;
    for condition in colors.iter().flat_map(|color| &color.conditions) {
        let tested = equality_subject(condition)?;
        if subject.is_some_and(|known| known != tested) {
            return None;
        }
        subject = Some(tested);
    }
    subject
}

/// The field of a `field == literal` row, in either operand order.
fn equality_subject(condition: &ConditionShape) -> Option<&str> {
    let ConditionShape::Row {
        left,
        operator: ComparisonOperator::Equal,
        right,
    } = condition
    else {
        return None;
    };
    match (left, right) {
        (Operand::Field { name }, Operand::Literal { .. })
        | (Operand::Literal { .. }, Operand::Field { name }) => Some(name),
        _ => None,
    }
}

/// The values a `choice` field declares, in order; none for any other
/// field.
fn declared_values<'schema>(schema: &'schema Schema, field_name: &str) -> &'schema [String] {
    match schema
        .fields
        .get(field_name)
        .map(|field| &field.type_config)
    {
        Some(FieldTypeConfig::Choice { values }) => values,
        _ => &[],
    }
}

/// A color value as resolved `#rrggbb`; `None` for any other value.
fn hex_of(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::Color(canonical) => resolve_color_to_hex(canonical),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::views::{ColorRole, DisplayConfig, ViewKind};
    use crate::parser::schema::parse_schema;
    use crate::view_data::test_support::make_store_with_files;
    use crate::view_data::{table, tree};

    const GREEN: &str = "#22c55e";
    const BLUE: &str = "#3b82f6";
    const GRAY: &str = "#6b7280";
    const RED: &str = "#ef4444";

    const STATUS_SCHEMA: &str = "\
fields:
  status:
    type: choice
    values: [to_do, in_progress, done, removed]
  parent:
    type: link
    allow_cycles: false
  color:
    type: color
    when:
      - if: status == \"done\"
        then: green
      - if: status == \"removed\"
        then: gray
      - if: status == \"in_progress\"
        then: blue
    default: gray
";

    fn view_of(kind: ViewKind) -> View {
        View {
            id: "v".to_owned(),
            where_clauses: Vec::new(),
            sort_keys: vec![],
            display: DisplayConfig::default(),
            kind,
        }
    }

    fn item_file(frontmatter: &str) -> String {
        format!("---\n{frontmatter}---\n")
    }

    /// The legend of a table over `files` under `schema_yaml`.
    fn table_legend(schema_yaml: &str, files: &[(&str, &str)], view: &View) -> Option<ColorLegend> {
        let schema = parse_schema(schema_yaml).expect("schema parses");
        let contents: Vec<(&str, String)> = files
            .iter()
            .map(|(name, frontmatter)| (*name, item_file(frontmatter)))
            .collect();
        let (_directory, store) = make_store_with_files(
            &schema,
            contents
                .iter()
                .map(|(name, content)| (*name, content.as_str()))
                .collect(),
        );
        let data = ViewData::Table(table::extract_table(view, &store, &schema));
        color_legend(view, &data, &store, &schema)
    }

    #[test]
    fn a_when_field_testing_one_field_lists_the_colors_on_screen_by_value() {
        let legend = table_legend(
            STATUS_SCHEMA,
            &[
                ("a.md", "status: done\n"),
                ("b.md", "status: done\n"),
                ("c.md", "status: to_do\n"),
                ("d.md", "status: removed\n"),
            ],
            &view_of(ViewKind::Table),
        )
        .expect("a when field has a legend");

        assert_eq!(legend.field, "color");
        // Blue is in the recipe and on no item, so it is absent. Gray
        // comes from a branch and from the default: one entry.
        assert_eq!(
            legend.meaning,
            ColorMeaning::ByValue {
                subject: "status".to_owned(),
                entries: vec![
                    ValueEntry {
                        color: GREEN.to_owned(),
                        count: 2,
                        values: vec![Some("done".to_owned())],
                    },
                    ValueEntry {
                        color: GRAY.to_owned(),
                        count: 2,
                        values: vec![Some("to_do".to_owned()), Some("removed".to_owned())],
                    },
                ],
                written: vec![],
            }
        );
    }

    #[test]
    fn an_entry_names_the_values_on_screen_not_the_branch_that_shares_its_color() {
        // Gray is `removed` and the default. No removed item is in the
        // view, so the entry must not read as if one were: it names
        // what the gray items are, an unset status last.
        let legend = table_legend(
            STATUS_SCHEMA,
            &[("a.md", "parent: b\n"), ("b.md", "status: to_do\n")],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        let ColorMeaning::ByValue { entries, .. } = legend.meaning else {
            panic!("conditions on one field are labelled by value");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].color, GRAY);
        assert_eq!(entries[0].values, vec![Some("to_do".to_owned()), None]);
    }

    #[test]
    fn a_hand_written_color_is_counted_apart_from_the_conditions() {
        // `b` is in progress and would derive blue; it wears a written
        // blue instead, which no condition explains.
        let legend = table_legend(
            STATUS_SCHEMA,
            &[
                ("a.md", "status: in_progress\n"),
                ("b.md", "status: in_progress\ncolor: blue\n"),
                ("c.md", "status: done\ncolor: red\n"),
            ],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        assert_eq!(
            legend.meaning,
            ColorMeaning::ByValue {
                subject: "status".to_owned(),
                entries: vec![ValueEntry {
                    color: BLUE.to_owned(),
                    count: 1,
                    values: vec![Some("in_progress".to_owned())],
                }],
                written: vec![
                    ColorCount {
                        color: BLUE.to_owned(),
                        count: 1,
                    },
                    ColorCount {
                        color: RED.to_owned(),
                        count: 1,
                    },
                ],
            }
        );
    }

    #[test]
    fn conditions_that_share_no_subject_are_listed_by_condition() {
        // The one branch is not a plain comparison, so it travels as
        // text; gray is the default's alone and has no condition.
        let schema = "\
fields:
  effort:
    type: integer
  budget:
    type: integer
  color:
    type: color
    when:
      - if: effort * 2 > budget
        then: red
    default: gray
";
        let legend = table_legend(
            schema,
            &[
                ("a.md", "effort: 5\nbudget: 8\n"),
                ("b.md", "effort: 1\nbudget: 8\n"),
            ],
            &view_of(ViewKind::Table),
        )
        .expect("legend");

        assert_eq!(
            legend.meaning,
            ColorMeaning::ByCondition {
                entries: vec![
                    ConditionEntry {
                        color: RED.to_owned(),
                        count: 1,
                        conditions: vec![ConditionShape::Text {
                            source: "effort * 2 > budget".to_owned(),
                        }],
                        otherwise: false,
                    },
                    ConditionEntry {
                        color: GRAY.to_owned(),
                        count: 1,
                        conditions: vec![],
                        otherwise: true,
                    },
                ],
                written: vec![],
            }
        );
    }

    #[test]
    fn a_pulled_color_is_a_hint_at_where_it_comes_from() {
        let schema = "\
fields:
  parent:
    type: link
    allow_cycles: false
  color:
    type: color
    pull:
      over: parent
      field: color
";
        let legend = table_legend(
            schema,
            &[("root.md", "color: red\n"), ("child.md", "parent: root\n")],
            &view_of(ViewKind::Table),
        )
        .expect("a pulled color has a hint");

        assert_eq!(
            legend.meaning,
            ColorMeaning::Inherited {
                over: "parent".to_owned(),
                source_field: "color".to_owned(),
            }
        );
    }

    #[test]
    fn a_color_field_without_a_recipe_has_no_legend() {
        let schema = "\
fields:
  color:
    type: color
";
        assert_eq!(
            table_legend(
                schema,
                &[("a.md", "color: red\n")],
                &view_of(ViewKind::Table)
            ),
            None
        );
    }

    #[test]
    fn an_untinted_view_has_no_legend() {
        let mut view = view_of(ViewKind::Table);
        view.display.color = Some(ColorRole::None);
        assert_eq!(
            table_legend(STATUS_SCHEMA, &[("a.md", "status: done\n")], &view),
            None
        );
    }

    #[test]
    fn a_view_whose_items_have_no_color_has_no_legend() {
        let schema = "\
fields:
  status:
    type: choice
    values: [to_do, done]
  color:
    type: color
    when:
      - if: status == \"done\"
        then: green
";
        assert_eq!(
            table_legend(
                schema,
                &[("a.md", "status: to_do\n")],
                &view_of(ViewKind::Table)
            ),
            None
        );
    }

    #[test]
    fn nested_items_of_a_tree_are_counted() {
        let schema = parse_schema(STATUS_SCHEMA).expect("schema parses");
        let (_directory, store) = make_store_with_files(
            &schema,
            vec![
                ("root.md", "---\nstatus: done\n---\n"),
                ("child.md", "---\nstatus: done\nparent: root\n---\n"),
                ("grandchild.md", "---\nstatus: done\nparent: child\n---\n"),
            ],
        );
        let view = view_of(ViewKind::Tree {
            field: "parent".to_owned(),
        });
        let data = ViewData::Tree(tree::extract_tree(&view, &store, &schema));

        let legend = color_legend(&view, &data, &store, &schema).expect("legend");
        let ColorMeaning::ByValue { entries, .. } = legend.meaning else {
            panic!("conditions on one field are labelled by value");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].count, 3);
    }
}
