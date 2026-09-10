---
id: schema-rules-editor
status: to_do
parent: schema-editor-web
depends_on: [schema-page-read-view, schema-field-write-backend, schema-field-type-change]
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
