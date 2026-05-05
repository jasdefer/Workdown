---
id: item-mutations
type: milestone
status: in_progress
title: Item mutations
parent: phase-04-visualization
depends_on: [foundation]
start_date: 2026-05-02
end_date: 2026-05-06
duration: "5d"
---

Add the CLI subcommands that mutate items — exercised by the UI and usable standalone. Every UI mutation maps 1:1 to a command here.

## Goals

- Generic field mutation: `workdown set <id> <field> <value>`
- Shortcut for the board field: `workdown move <id> <value>`
- Audit `workdown add` for UI-driven item creation (templates, defaults, required fields)

## Note on naming

`render` and `serve` are also CLI commands but live in their feature milestones. This milestone collects only the commands the UI invokes as *mutations*.
