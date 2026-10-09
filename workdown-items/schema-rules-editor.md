---
id: schema-rules-editor
status: in_progress
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend]
title: Edit rules on the schema page
---

## In plain words

Rules say things like "an item in progress needs an assignee". The
schema page lists them read-only. This item makes them editable: add a
rule, change its match and require blocks, remove it.

**Last in the milestone's build order**, after every field-editor
child. Decided in [[schema-editor-web-design]] (decision 15): fields
first, so the rule form is designed against a real page. Its UX gets
its own decision sheet when it is picked up; the structured rule shape
it needs is then added to `GET /api/schema/definition` beside the text
[[schema-definition-api]] serves.

## What has to be settled when it is picked up

- **Match and require as forms.** Both are field → condition maps,
  with `children.status`-style paths across relations and the
  `any`/`all`/`none` quantifiers. A form that offers only fields and
  paths the schema has, and only conditions valid for the field's type.
- **Splice for rules.** The write backend splices `fields:` entries;
  rules are a sequence, so the splice addresses an entry by index or
  name. Same contract: everything else byte-identical.
- **Preview.** A new or tightened rule can warn on many items at
  once; the same preview dialog applies.

## Decisions taken

Settled on 2026-10-09 with the user. Three premises checked first:
a quantifier nested inside a quantifier parses but is skipped at
evaluation, so the form offers one level; the comparison operators
(`eq_field` and friends) are not checked against the field list at
schema load, so the form's picker is the only guard; and every rule in
this repo's own schema uses a quantifier, so a first cut without them
could edit none of them.

### Scope and surface

1. **Add, edit, remove; no reorder.** Rule order only affects the
   order of validation output.
2. **Rename allowed on edit.** Nothing references a rule name except
   validation output and the field usage list, so a rename rewrites
   nothing. The edit request carries the new name in the body and the
   old name in the path. Fields stay fixed because items and views
   name them; that reason does not apply here.
3. **Same docked panel, same URL state.** A rule entry opens the
   panel at `?rule=<name>`, "Add rule" opens it at `?add-rule`. Save
   closes it and its warnings stay above the list until the next save,
   as with fields. The read-only YAML pane takes the panel's second
   column, where the field panel shows usage.

### The rows

4. **A reference is two cells: anchor, then field.** Anchor is "this
   item" or a relation — every `link`/`links` field and every inverse
   the schema defines. Field is a field picker. `children` + `status`
   writes `children.status`; a relation anchor with no field is the
   bare reference the parser accepts for counts and `is_set`. Rejected:
   one flat select of every valid path, about a hundred entries here.
5. **Match operators follow the anchor.** Single-valued reference:
   equals, one of, not, is set. One-to-many reference: all, any, none,
   each with an inner operator from the same four. A many reference
   with no quantifier is valid YAML and means `all`; the form shows it
   as `all` and writes it back that way.
6. **Require assertions follow the target type.** `required` and
   `forbidden` always; `values` and `not` on any scalar type; the five
   comparisons only on date, integer, float and duration, with a picker
   of other fields of the same type plus `$today` on dates; `min_count`
   and `max_count` only on a one-to-many reference.
7. **Several rows on one reference merge into one object.** Two rows
   on `children.status`, `any: on_hold` and `none: in_progress`, write
   the object this repo's second rule already has. The same operator
   twice on one reference is a form error.
8. **Values through the existing value editor.** A row's value cell is
   `ValueEditor` with the target field's spec, so a choice value is a
   dropdown over its values and a date a date input; one-of and a
   not-list use the multichoice editor over the same values.
9. **Shorthand on write.** The writer emits the shortest form the
   parser accepts: a scalar for single equals, the `required` and
   `forbidden` keywords, a scalar in `require` for a one-value list.
   What the defaults and the docs show is what the editor writes.

### The rest of the rule

10. **Name, description, severity above the rows; count below** as one
    block, "Limit how many items may match", with min and max. Save is
    greyed out until the rule has a require row or a count (the
    parser's own rule) and the name is kebab-case and unused.
11. **A rule the rows cannot represent is shown, not edited.** The
    only case is a nested quantifier. The panel shows the YAML body
    read-only with a sentence saying why, and offers only Remove.

### The wire

12. **The grammar stays in Rust, derived from the existing rule
    types.** `GET /api/schema/definition` serves each rule's `match`,
    `require` and `count` as structured data beside the text body it
    serves today, converted from the parsed `Condition`, `Assertion`
    and `CountConstraint` with exhaustive matches, so a new operator
    fails to compile until the editor's shape names it. The write
    accepts the same structure back and the server turns it into the
    YAML tree. Types reach the browser through `cargo xtask gen-types`
    like every other wire type. Rejected: serving the generic mapping
    as JSON and letting the browser read scalar-versus-list-versus-
    object forms, a second copy of the grammar that drifts.
13. **The YAML pane and the pre-save verdict come from the server.**
    A debounced `POST /api/schema/rules/preview` takes the structured
    draft and returns the YAML text the write would produce plus the
    parser's verdict on the candidate schema. The pane shows the text;
    a refusal shows in the panel before Save. The user's reason: the
    server already has the parser and the writer, it is all local, and
    a preview produced by the same code as the save cannot drift from
    it. Rejected: a TypeScript serializer for the preview.
14. **Routes and splice mirror fields.** `POST /api/schema/rules`
    appends, `PUT /api/schema/rules/{name}` replaces, `DELETE
    /api/schema/rules/{name}` removes. The splice addresses the
    `rules:` sequence by name; everything else stays byte-identical.
    Last write wins per rule, no conflict detection, as fields decided.
15. **Save-then-warnings, nothing new.** A tightened rule flags items
    on save and they appear in the page's warnings list; no pre-save
    item count. To verify while building: that the write outcome's
    warnings include rule violations, not only the cross-file field
    checks.

### Build

16. **One item, full grammar.** Rust structured rule, writer and
    preview endpoint first; then the panel with the row components;
    then the preview wiring.

Not in this item: a raw YAML escape hatch in the form, and checking
comparison references on the server. The second is a real parser gap
and gets its own backlog entry.

## Built

**2026-10-09, the Rust half** (decision 16, step one). Uncommitted.

- `crates/core/src/rule_shape.rs`: the rows (`RuleShape`, `MatchRow`,
  `RequireRow`, `ScalarTest`, `RequireTest`, `Quantifier`,
  `Comparison`, `ScalarValue`), built from the parsed `Condition`,
  `Assertion` and `CountConstraint` with exhaustive matches, and
  written back as the YAML tree in the shortest form the parser reads.
  Refuses in both directions: a nested quantifier and
  `required: false` have no rows; an equality beside another test on
  one reference, or an operator twice, cannot be written.
- `GET /api/schema/definition`: each rule carries `shape`, either
  `{kind: rows, shape}` or `{kind: unrepresentable, reason}`, beside
  the text `body`.
- `operations/schema_write.rs`: `add_rule`, `update_rule` (rename
  allowed), `remove_rule`, `preview_rule`; the outcome and
  `SchemaMutationResult` gained `rule_name`.
- Routes: `POST /api/schema/rules`, `PUT`/`DELETE
  /api/schema/rules/{name}`, and `POST /api/schema/rule-preview`.
  *Deviation from decision 13:* the preview lives at
  `/schema/rule-preview`, not `/schema/rules/preview`, so a rule named
  `preview` stays addressable, the same reason `/schema/field-order`
  has its own address. The preview answers `200` with `problem` set
  for every draft-level fault (unwritable rows, bad or taken name,
  reference to no field) and `422` only when the existing file does
  not load.
- Decision 15 verified: the write outcome's warnings include rule
  violations (`add_rule_reports_the_items_it_now_flags`).

Next: the panel and the row components (step two), then the preview
wiring (step three).
