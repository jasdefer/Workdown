---
id: schema-field-editor-text
status: to_do
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
