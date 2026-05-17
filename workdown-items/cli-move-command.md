---
id: cli-move-command
type: issue
status: done
title: workdown move — shortcut for the board field
parent: item-mutations
depends_on: [cli-set-command]
---

Add a convenience command:

```
workdown move <id> <value>
```

Equivalent to `workdown set <id> <board_field> <value>`, where `board_field` comes from `config.yaml` (default: `status`).

## Rationale

Status changes are the most frequent mutation. A shorter command matches the common case. The UI's drag-drop on the board view will call this (or `set` directly — implementation detail, same effect).
