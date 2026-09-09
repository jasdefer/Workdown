---
id: schema-field-rename
status: on_hold
parent: schema-editor-web
depends_on: [schema-field-write-backend]
title: Rename a field and everything that names it
---

## In plain words

The field editor locks a field's name, because renaming it would
orphan every item, view slot, rule and expression that uses the old
name. A rename that is safe has to rewrite all of them in one go: the
frontmatter key in every item, the slots in `views.yaml`, the field
references in rules, the identifiers inside `compute` and `when`
expressions and the roles in `config.yaml`. That is a project-wide
refactor, not a schema edit, which is why it is its own item.

**Parked until** the plain-field editor has shipped and someone asks
for it. Decision 5 of [[schema-editor-web-design]].

## What has to be settled when it is picked up

- Which files are rewritten and how each is spliced without losing
  comments — items already go through `frontmatter_io`, views and the
  schema through their splices, config is untouched territory.
- Whether `config.yaml` is rewritten at all, given it is read at boot.
- The preview: a list of every file the rename touches, before it
  happens.
- Whether the same machinery serves renaming a *value* of a choice
  field with item rewrite, raised in [[schema-field-editor-values]].
