---
id: virtual-id-in-query-eval
status: done
title: Resolve the virtual `id` in query evaluation and sorting
parent: polish
---

Filtering and sorting by `id` silently does nothing. Verified against a
scratch project containing `alpha.md`:

- `workdown query --where "id=alpha"` matches **no** items —
  `eval_comparison` (`query/eval.rs`) resolves `Local` references via
  `item.fields.get(name)`, and the virtual `id` never appears in the
  fields map.
- `--sort id` only *appears* to work ascending because the deterministic
  id tie-breaker in `sort_items` (`query/sort.rs`) kicks in after every
  spec compares missing-vs-missing; `--sort id:desc` does not reverse.

The same evaluation path backs a view's `where:` clauses, so
`where: [id=…]` in `views.yaml` is equally dead. `views_check`
deliberately keeps accepting `id` in where clauses (see
[[virtual-id-in-structural-slots]]) because filtering by id is
legitimate — this item makes it actually work.

## Root cause

`parse_work_item` uses `frontmatter.remove("id")`, so the id is lifted
out of the frontmatter map into `RawWorkItem.id` whether it came from an
explicit `id:` key or from the filename. Both kinds of item are equally
affected — there is no case where `id` survives into `item.fields`.

## Decisions taken (2026-07-31)

1. **Project the id into the field map** at coercion, rather than
   special-casing the name at each read site. `coerce_fields` inserts
   `id → FieldValue::String(raw.id)` before the schema loop. Filtering,
   sorting, `where:` clauses, and relation paths then work with no
   id-aware code in any of them.
2. **The parser's `remove` stays.** It is what normalizes "filename or
   frontmatter key" into a single authoritative `raw.id`. Leaving `id`
   in the frontmatter map instead would give two sources, let a schema
   declaration re-type the id (`type: integer` would be honoured), and
   fire the unknown-field warning for schemas that don't declare `id`.
   The obsolete line was `coerce.rs`'s `if name == "id" { continue; }`,
   which dug the hole; it now guards against reprocessing instead, so a
   schema marking `id` required cannot raise a spurious
   `MissingRequired`.
3. **The projection ignores its schema declaration** entirely — always a
   string, present whether or not the schema declares `id`. The id is a
   string by construction; honouring a declared type would be a lie.
4. **Precedent:** `derive::run` already mutates `items` in place with
   compute and aggregate values that exist in no file, explicitly so
   consumers "see manual + derived values indistinguishably". The field
   map is already a read projection, not a mirror of the file. Writes
   are unaffected because every write path re-parses raw frontmatter
   (`operations/frontmatter_io.rs`) and never consults `item.fields`.
5. **`parent.id` / `children.id` came for free** — `resolve_field_ref`
   reads `target.fields.get(field_name)`, so relation paths resolve the
   projection on the target item. The out-of-scope question in the
   original write-up is answered: no extension was needed. Forward
   `parent.id=x` differs from `parent=x` only on broken references (it
   matches existing targets only); inverse `children.id=x` is newly
   expressible.
6. **Cards must not grow an `id` row.** `build_card` skipped `id` via
   `schema.fields.get(name)` returning `None` — but the shipped default
   schema *does* declare `id`, so that guard never fired for default
   projects; the empty field map was doing the work. It now skips by
   name, keeping the id the card's identity rather than a field row.
   Rendering is unchanged.
7. The pre-existing id special-cases in `query/engine.rs` (`build_row`)
   and `view_data/common.rs` (`column_cell`, `build_column`) are left
   in place. They are now defensive rather than load-bearing, and they
   keep hand-constructed items (tests, other consumers) rendering an id.

## Reframing

[[virtual-id-in-structural-slots]] rejects `id` in structural slots.
Those rejections are by name and still hold, but their justification
shifts: not "the slot reads a field that is never there", but "grouping
or plotting by a unique key is meaningless". The diagnostic and the
behaviour are unchanged.

## Verification

Unit tests cover the projection (including a schema that declares `id`
as required/integer, and a schema that omits it), the filter operators,
both sort directions, the relation paths, and the card guard. Verified
end-to-end against a scratch project: `id=`, `id in`, `id/regex/`,
`id~contains`, `id!=`, `--sort id`, `--sort id:desc` (the case that
never reversed), `parent.id=`, an item whose id comes from a frontmatter
key, and that `set`/`rename` still write no `id:` key into a file that
had none while preserving an explicit one exactly once.
