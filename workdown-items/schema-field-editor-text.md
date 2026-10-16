---
id: schema-field-editor-text
status: done
parent: schema-editor-web
depends_on: [schema-field-editor-shell]
title: Field editor block for string and list, with the resource picker
---

## In plain words

A text field can be constrained two ways: a pattern its value must
match, or a resource it must name — an assignee has to be someone in
`resources.yaml`, not any string. This item adds the block that sets
either.

One of the five type-specific shapes in [[schema-editor-web-design]].

## What is needed

- **Pattern** (string only): a text input; the server rejects an
  invalid regex at parse time, and the panel shows that error where
  the input is.
- **Resource** (string and list): a picker over the resource lists the
  definition payload carries from `resources.yaml`. Picking one sets
  `resource: <name>`; clearing removes the key.
- The two are mutually exclusive in the parser today; the block
  enforces that in the UI.

## Not in scope

- Editing resources themselves. That is a resources editor, not
  scheduled.

## Decisions taken

Settled on 2026-10-06 with the user.

1. **Pattern and resource are not mutually exclusive.** The item
   assumed the parser enforces that; it does not. A `string` field may
   carry both, and a value then has to match the pattern and be an
   entry of the list. The block edits them independently and adds no
   rule the server does not have.
2. **The resource names come from the schema store**, the payload of
   `GET /api/schema` the panel already reads for the default control,
   not from the definition payload as the item assumed. That payload
   describes `schema.yaml` and the type system; what lists exist is a
   fact about `resources.yaml`, and the store already carries the
   lists with their entries. Adding the names to the definition
   payload would serve the same information twice. The store gained a
   `resourceNames` getter. `resources.yaml` is only read here; editing
   the lists stays out of scope.
3. **`resource` stays a property of the field, not of its shape**, as
   the data model has it: it spans `string` (shape `text`) and `list`
   (shape `scalar`). The UI shows it as its own `ResourceControl`
   under the shape block whenever the type table allows it, and the
   panel renders one line for it, as for the shape block.
4. **The picker is a select** over the list names, with a blank choice
   for none, and a name the file holds that no list carries stays
   selectable, marked unknown — the pattern the value editor uses for
   stray values — so the panel shows what the file says and a save
   does not silently drop it.
5. **No regex check in the browser.** The item wanted the server's
   error "where the input is". A pattern is a regex in Rust's dialect;
   the browser's `RegExp` accepts and refuses different things, so a
   client check would mislead. The server refuses with its message,
   shown at the top of the panel like every other refusal; it names
   the property.
6. **Pattern is a monospace text input,** written as typed; blank
   removes the key. On a type change from `string` to `list` the
   resource is kept and the pattern goes with the shape, as `retype`
   already did.
7. **`ShapeBlock` dispatches the type-specific block.** With the
   second block the panel would have grown a branch, a derived value
   and a handler per shape. The panel now renders one `ShapeBlock`,
   which switches on the shape's kind, and one `shapeProblem(shape)`
   judges every shape in one place (replacing `boundsProblem`).
