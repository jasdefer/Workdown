---
id: config-field-role-validation
status: done
title: Validate the field-role keys in config.yaml against the schema
parent: time-tracking
---

## In plain words

The keys in `config.yaml` that name which field plays which role should
be checked against the schema when the project is validated, so a typo
in one is reported instead of sitting in the file unnoticed.

A project names a field for each structural role it needs —
`board_field`, `tree_field`, `graph_field` — and nothing checks that
those fields exist or have the right type. `workdown move` catches its
own case with a clear message when you run it; the other two catch
nothing, because nothing reads them at all. **Example:**
`tree_field: parnet` produces no complaint from `workdown validate`,
and never will from anywhere else.

## Why this belongs in workdown

The gap is already written down as a gap: the config validator's own
opening comment says validating the structural defaults "is a separate
concern — those fail loudly at use time — and would grow its own checks
here if it lands." The neighbouring `defaults.display` roles are checked
exactly this way, so the machinery, the diagnostic scoping and the
non-blocking convention all exist; only these keys were left out.

Two of the three keys are read by no code anywhere — not the CLI, not
the server, not the web app; `board_field` in `workdown move` is the
only consumer of any of them. That sounds like an argument for leaving
them alone, and it is the opposite. They are mandatory keys on a struct
that rejects unknown fields, so they cannot be removed without breaking
the parse of every existing project's config: they will sit in every
`config.yaml` forever, read by nothing. Checking them is the only thing
that makes them honest. Whether they should eventually gain a consumer
or be documented as decoration is a separate question, and this check
lands either way.

It becomes worth doing now because [[effort-field-config]] adds a fourth
such key whose consumer cannot fail loudly. A timer in the web app has
no command to run and no error to print — an unset effort field is a
legitimate state meaning "no timer", so a typo and a deliberate blank
look the same. That key needs the eager check, and building it for one
key alone would leave the three that already exist unchecked.

## Decisions taken

1. **The three keys that exist, plus the mechanism the fourth plugs
   into.** `board_field`, `tree_field` and `graph_field` are checked
   here, against a role table with one row per key.
   [[effort-field-config]] adds `effort_field` as a row alongside the
   key itself, so the key is never in a release unvalidated. Not "all
   four here": the fourth key does not exist yet, and introducing it
   here would take the substance out of the item that is about it.
2. **Same treatment the display defaults already get:** reported
   through `workdown validate` and the serve banner, pinned to
   `config.yaml`, and project-wide — carrying no `view_id`, so no
   single view is ever marked unrenderable. One config typo should not
   blank a view, let alone every view at once.
3. **Warning, not error.** The display defaults are errors because a
   bad value there silently makes every view render wrong. Nothing here
   renders wrong: `workdown move` prints its own message when you run
   it, the tree and graph keys feed nothing, and a misnamed effort field
   means no timer. That is advice, not damage, so `workdown validate`
   keeps exiting zero — and "non-blocking" means one thing (views keep
   rendering *and* validation stays green) rather than two. It also
   means a project that upgrades does not fail its pipeline over a
   config line nobody reads.
4. **Existence and type, mirroring the matching view slot exactly.**
   `board_field`: `choice`, `multichoice` or `string`. `tree_field`:
   `link`. `graph_field`: `link` or `links`, inverse names included.
   `effort_field`, when it arrives: `duration`. Deliberately not
   stricter than what the tool already accepts — a board view takes all
   three of those types, and `workdown move` type-checks nothing at
   all, so a `string` board field works today and must not be reported
   as if it were broken. Whether the field is computed, aggregated or
   pulled is not this check's business.
5. **A key naming `id` gets its own diagnostic.** "Unknown field 'id'"
   would be a lie — the id resolves everywhere, and some projects
   declare it in the schema besides. Reporting it as a type mismatch
   was the first plan, borrowed from how the `color` display role
   handles `id`, and it does not survive decision 4: the board role
   accepts `string`, so `board_field: id` would produce either silence
   or the nonsense "has type string, expected choice, multichoice, or
   string". So the config scope gets `ConfigVirtualIdNotAllowed`,
   mirroring the trio the view slots already have, and the id is
   rejected by name before the schema is consulted — as `check_slot`
   does — so the verdict does not depend on whether a project declares
   it.
6. **Use-time errors stay.** `workdown move` keeps its own message; the
   eager check is an addition, not a replacement, because a config can
   change between validating and running.
7. **One generalised diagnostic pair, not a second parallel one.**
   `ConfigDisplayUnknownField` and `ConfigDisplayFieldTypeMismatch`
   already carry the slot path as a static string, so they become
   `ConfigUnknownField` and `ConfigFieldTypeMismatch` and take
   `defaults.board_field` as readily as `defaults.display.title`. The
   JSON tag names change with them, as they did on the views side
   ([[diagnostic-variant-cleanup]]); one test names the old variant and
   no diagnostic-routing match does. The role table stays private to
   `config_check`: the view-side type sets vary per view kind, so there
   is no single shared rule to extract the way `display_check` shares
   the display-role rules.
8. **The check verifies itself against bugs we already have.** Four
   test fixtures promise fields their schemas never define — the server
   project fixture and the one in `views_write_endpoint` both say
   `graph_field: depends_on` with no `links` field anywhere, and the
   core `resource_refs` and `validate_views` fixtures name all three
   roles against schemas holding one field. Making each fixture
   internally consistent is part of this item, and is the observable
   result it would otherwise lack until a timer exists.
9. **No hint about a missing effort field.** An earlier decision here
   had `workdown validate` suggest configuring one whenever a duration
   field existed. Dropped: every diagnostic the tool has describes
   something the user wrote that is wrong, this one would describe
   something they did not write, and there would be no way to say "I
   know, I do not want a timer" — so a project using durations for
   calendar planning would carry the warning forever, which is exactly
   the normal project ([[effort-field-config]] decision 3). The
   discoverability answer belongs where someone is actually looking for
   a timer: the empty state in the app where the timer would be
   ([[effort-timer]]).
10. **No ADR.** A fourth instance of the pattern ADR-008 and the three
    existing keys already establish.
11. **Filed under [[time-tracking]]** because that is the milestone
    pulling it in, not because it is about time. It stands on its own
    and would be worth doing regardless.

## Not in scope

- Making any of the keys mandatory or optional differently than they
  are today.
- Giving `tree_field` and `graph_field` a consumer, or taking them
  away. Both are checked here regardless of what happens to them next.
- A JSON Schema for `config.yaml`. It is the one config file with no
  formal schema shipped for editor autocomplete, which is why these keys
  are easy to typo in the first place — but that is its own piece of
  work.
