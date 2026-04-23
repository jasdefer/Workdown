---
id: cli-add-audit
type: issue
status: to_do
title: Audit workdown add for UI-driven creation
parent: item-mutations
---

Confirm `workdown add` supports everything the UI will need to create items from a browser form.

## Checks

- All schema field types settable via flags (string, choice, multichoice, integer, float, date, boolean, list, link, links)
- Default generators (`$today`, `$uuid`, `$filename`, `$filename_pretty`, `$max_plus_one`) behave correctly
- Templates usable to pre-fill an add call (`--template <name>`)
- Clear error surface when a required field is missing
- `add` exposes a programmatic function in `core` that takes a struct (not just argv), so the server can build one from a JSON body

## Deliverable

Either "nothing to do, already works" documented here, or a list of specific gaps filed as sub-issues.
