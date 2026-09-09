---
id: schema-rules-editor
status: on_hold
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend]
title: Edit rules on the schema page
---

## In plain words

Rules say things like "an item in progress needs an assignee". The
schema page lists them read-only. This item makes them editable: add a
rule, change its match and require blocks, remove it.

**Parked until** the field editor has shipped. Decided in
[[schema-editor-web-design]]: fields first, so the rule editor is
designed against a real page.

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
