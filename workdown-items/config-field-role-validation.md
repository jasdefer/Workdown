---
id: config-field-role-validation
status: to_do
title: Validate the field-role keys in config.yaml against the schema
parent: time-tracking
---

## In plain words

The keys in `config.yaml` that name which field plays which role should
be checked against the schema when the project is validated, instead of
only blowing up later at the moment something tries to use them.

A project names a field for each structural role it needs —
`board_field`, `tree_field`, `graph_field` — and today nothing checks
that those fields exist or have the right type until a surface reaches
for one. `workdown move` catches its own case with a clear message, but
the tree and graph keys have no equivalent, and a typo in either sits in
the file unreported. **Example:** `tree_field: parnet` produces no
complaint from `workdown validate` at all.

## Why this belongs in workdown

The gap is already written down as a gap: the config validator's own
opening comment says validating the structural defaults "is a separate
concern — those fail loudly at use time — and would grow its own checks
here if it lands." The neighbouring `defaults.display` roles are checked
exactly this way, so the machinery, the diagnostic scoping and the
non-blocking convention all exist; only these keys were left out.

It becomes worth doing now because [[effort-field-config]] adds a fourth
such key whose consumer cannot fail loudly. A timer in the web app has
no command to run and no error to print — an unset effort field is a
legitimate state meaning "no timer", so a typo and a deliberate blank
look the same. That key needs the eager check, and giving it to one key
alone would make the odd one out of the key that is behaving correctly.

## Decisions taken

1. **All four keys, together.** `board_field`, `tree_field`,
   `graph_field` and `effort_field` — one check over the field-role
   keys, not a special case for the newest one.
2. **Same treatment the display defaults already get:** reported through
   `workdown validate` and the serve banner, pinned to `config.yaml`,
   and non-blocking. A bad key is reported and everything keeps
   rendering on whatever fallback it has; one config typo should not
   blank every view at once.
3. **Existence and type, nothing more.** Each key's field must be
   defined in the schema and be of the type its role needs — `choice`
   for a board, `link` for a tree, `links` for a graph, `duration` for
   effort. Whether the field is computed, aggregated or pulled is not
   this check's business.
4. **Use-time errors stay.** `workdown move` keeps its own message; the
   eager check is an addition, not a replacement, because a config can
   change between validating and running.
5. **A hint when a duration field exists and no effort field is set.**
   Reported the same non-blocking way, saying that timer surfaces are
   off. This is the discoverability answer for a key that appears in no
   shipped default and no editor autocomplete — without the tool
   guessing which field it is ([[effort-field-config]] decision 3).
6. **The diagnostic shape is settled as part of the work.** Every config
   diagnostic today is display-role-specific — "unknown field for slot
   *title*" — so a field-role check needs either its own pair of
   variants or a generalisation of the existing pair. The same
   duplication [[diagnostic-variant-cleanup]] collapsed on the views
   side, so prefer the generalisation.
7. **Filed under [[time-tracking]]** because that is the milestone
   pulling it in, not because it is about time. It stands on its own and
   would be worth doing regardless.

## Not in scope

- Making any of the keys mandatory or optional differently than they
  are today.
- A JSON Schema for `config.yaml`. It is the one config file with no
  formal schema shipped for editor autocomplete, which is why these keys
  are easy to typo in the first place — but that is its own piece of
  work.
