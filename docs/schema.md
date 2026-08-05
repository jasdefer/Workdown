# Schema Reference

The schema file (`.workdown/schema.yaml`) defines the fields on your work items and the validation rules that enforce constraints across them. It is copied into your project by `workdown init` and is fully customizable.

The formal structure of `schema.yaml` is defined in `schema.schema.json` (JSON Schema). This document explains how to use it.

## Fields

The `fields:` section defines what metadata your work items have. Each field has a name (the key) and a definition that picks a type and configures it.

```yaml
fields:
  priority:
    type: choice
    values: [critical, high, medium, low]
    required: false
```

Field names are lowercase letters, digits, and underscores, starting with a letter or underscore. The name `none` is reserved — display roles in `views.yaml`/`config.yaml` use it as their "no field" sentinel (e.g. `display: { color: none }`), so a field by that name could never be referenced there.

### Available types

| Type | Description | Type-specific options |
|------|-------------|---------------------|
| `string` | Free text | `pattern` (regex) |
| `choice` | Pick one from a list | `values` (required) |
| `multichoice` | Pick zero or more from a list | `values` (required) |
| `integer` | Whole number | `min`, `max` |
| `float` | Decimal number | `min`, `max` |
| `date` | Calendar date (YYYY-MM-DD) | |
| `duration` | Length of time (`5d`, `1w 2d 3h`, `30min`) | `min`, `max` (duration strings) |
| `color` | Hex color (`#rgb` / `#rrggbb`) or a built-in palette name (`red`, `orange`, `yellow`, `green`, `blue`, `purple`, `pink`, `gray`) | |
| `boolean` | True or false | |
| `list` | List of free-text strings | |
| `link` | Single reference to another work item | `allow_cycles`, `inverse` |
| `links` | Multiple references to other work items | `allow_cycles`, `inverse` |

### Common options (all types)

| Option | Type | Description |
|--------|------|-------------|
| `type` | string | Required. One of the types above. |
| `required` | boolean | Whether the field must be present. Default: `false`. |
| `default` | value | Default value applied by `workdown add`. Can be a literal or a generator. |
| `description` | string | Human-readable explanation. |
| `resource` | string | Name of a resource section in `resources.yaml`. Valid for `string` and `list` types. See [Resources](#resources). |
| `aggregate` | object | Aggregation config for computed fields (see below). |
| `compute` | string or object | Derive the value from an expression (see [Computed fields](#computed-fields)). |
| `when` | array | Derive the value by first matching condition (see [Conditional fields](#conditional-fields)). Next to `when`, `default` is the evaluated fallback, never written to files. |
| `inverse` | string | Inverse relationship name for rules dot-notation. Only valid for `link` and `links` types. See [Rules](#rules). |

### Generators

Fields can have generated defaults, applied when creating a new work item with `workdown add`:

| Generator | Description | Valid for types |
|-----------|-------------|-----------------|
| `$filename` | Filename without `.md` extension | string |
| `$filename_pretty` | Filename converted to title case | string |
| `$uuid` | Random UUID | string |
| `$today` | Current date | date |
| `$max_plus_one` | Highest existing value + 1 | integer |

### Aggregated fields

Fields with an `aggregate` config are set manually on leaf items and computed automatically up the parent chain.

```yaml
fields:
  estimated_hours:
    type: integer
    required: false
    aggregate:
      function: sum
      error_on_missing: false
```

| Option | Description |
|--------|-------------|
| `function` | The aggregation function. See table below. |
| `over` | The `link` field whose hierarchy the rollup climbs. Default: `parent`. |
| `error_on_missing` | Whether to report an error if a leaf item is missing this field. Default: `false`. |

Available aggregate functions by type:

| Type | Functions |
|------|-----------|
| `integer`, `float`, `duration` | `sum`, `min`, `max`, `average`, `median`, `count` |
| `date` | `min`, `max`, `average` |
| `boolean` | `all`, `any`, `none`, `count` |

If two items in the same ancestor chain both define the value manually, it is a validation error.

### Computed fields

Fields with a `compute` config derive their value from an expression over the *same item's* other fields — the cross-field counterpart to aggregation (which is cross-item, same field):

```yaml
fields:
  end_date:
    type: date
    compute: start_date + duration

  cost:
    type: float
    compute: effort / $constants.work_hours_per_day * $constants.daily_rate

  finish:
    type: date
    compute:
      expression: start_date + effort
      round: ceil
      error_on_missing: true
```

`compute` is either the expression string directly, or a mapping with options:

| Option | Description |
|--------|-------------|
| `expression` | The expression. Field names, `$constants.<name>` references ([constants](#constants) from `resources.yaml`), `$today`, numeric literals, quoted string literals (`"done"`), `true`/`false`, arithmetic (`+ - * /`), comparisons (`== != < <= > >=`), and parentheses. |
| `round` | For date results with a sub-day remainder: `nearest` (default), `floor` (the last fully-used day), or `ceil` (the day the work spills into). Only valid on `date` fields. |
| `error_on_missing` | Report an error when an item is missing an expression input, instead of silently leaving the field absent. Default: `false`. |

Computed values are never written to files. They are derived at load time and visible everywhere — `workdown query`, every view, rules — indistinguishable from set values. A value written in frontmatter always wins; compute fills only absent fields.

`compute` is only valid on `integer`, `float`, `date`, `duration`, and `boolean` fields, and cannot be combined with `default`. Expressions are type-checked at load time against a closed algebra:

| Expression | Result type |
|-----------|-------------|
| `date ± duration` | `date` |
| `date - date` | `duration` |
| `duration ± duration` | `duration` |
| `duration * number`, `duration / number` | `duration` |
| `duration / duration` | `float` |
| `integer op integer` | `integer` (except `/`, which is always `float`) |
| mixed number arithmetic | `float` |
| `number cmp number`, `date cmp date`, `duration cmp duration` | `boolean` (`cmp` is any of `== != < <= > >=`) |
| `text == text`, `boolean == boolean`, `color == color-or-text` | `boolean` (equality and `!=` only) |

Comparisons follow the same strictness as the arithmetic: `duration < 5` is an error (5 of what — hours? days?), and ordering a `choice` or `string` is meaningless and rejected. String literals are always quoted (`status == "done"`), so a typo'd field name stays an unknown-field error instead of silently becoming text; `true` and `false` are reserved words. At most one comparison per expression — there are no `and`/`or` combinators; multi-condition logic is expressed by ordering `when:` branches (first match wins). Color equality compares resolved hex, so `tint == "red"` matches whether the field holds the palette name or its hex.

```yaml
fields:
  is_overdue:
    type: boolean
    compute: end_date < $today
```

Everything else — unknown references, `date + date`, a result type that doesn't fit the declared field type, expressions referencing each other in a cycle — is reported when the project loads.

`$today` is the current date as a `date`, resolved once per run (ADR-010):

```yaml
fields:
  days_remaining:
    type: duration
    compute: end_date - $today
```

An expression using `$today` makes derived values — and any rendered views built from them — depend on the day the command runs. Every evaluating command (`validate`, `query`, `render`, `serve`) takes `--as-of <YYYY-MM-DD>` to pin the date, so a given commit produces identical output on any day; `workdown render` prints a notice when a computed field reads the clock. Note the distinction from `default: $today`, which resolves once at `workdown add` time and writes a literal date into the file.

Computed fields may reference other computed fields (evaluation runs in dependency order), and compose with aggregation:

- A field with **only** `compute` evaluates on every item whose inputs resolve — including items whose inputs were themselves rolled up. `flow_efficiency: effort / duration` on a milestone is `sum / sum`.
- A field with **both** `compute` and `aggregate` computes on *leaf* items only; the aggregate fills everything above. `end_date` on a milestone is the `max` of its children's ends — not its rolled-up `start + duration`, which would be blind to gaps between children.

Per-item problems are reported without blocking the load: missing inputs (with `error_on_missing` or on a `required` computed field, naming the inputs that are absent) and runtime failures such as division by zero.

### Conditional fields

Where `compute` calculates a value, `when` *chooses* one: a list of branches checked top to bottom, and the first condition that holds supplies the value.

```yaml
fields:
  urgency_color:
    type: color
    when:
      - if: status == "done"
        then: green
      - if: end_date < $today
        then: red
    default: gray
```

- **First match wins.** Branch order is meaning: put the most specific condition first. There are no `and`/`or` combinators — conjunctions are expressed by bailing out on complements in earlier branches, disjunctions by two branches with the same `then`.
- **`if`** is a boolean expression in the same grammar as `compute` (comparisons, `$today`, `$constants.<name>`, quoted string literals). Each condition must type-check as boolean; a broken one is a single diagnostic against `schema.yaml` naming the field and branch number, and disables the field — never one error per item.
- **`then`** is a literal of the field's declared type, validated at load.
- **A branch whose condition cannot be answered** — a referenced field is absent on the item — does not match, and evaluation falls through to the next branch, mirroring how rules skip comparisons on absent operands.
- **`default`** next to `when` is the *evaluated* fallback when no branch matches. Unlike an ordinary add-time default it is never written into any file (a stamped value would permanently shadow every branch), and it must be a plain literal, not a generator.
- With no match and no `default` the field stays unset — on a `required` field that is a per-item diagnostic reporting that no branch matched, naming any condition inputs absent on the item.

Conditional fields compose like computed ones: `when` and `compute` are mutually exclusive on one field, but `when` + `aggregate` means conditions fill leaf items and the rollup fills ancestors. Conditions may read computed fields and vice versa — all derived fields share one dependency graph, evaluated in reference order, with cycles rejected at load. Values are derived at load, visible everywhere, and a hand-written frontmatter value always wins.

`when` is not supported on `link` and `links` fields: relations (reverse links, tree structure, broken-reference checks) are built from hand-written values, so a derived link would be a phantom edge. Declaring one is a schema error.

### Pull fields

Fields with a `pull` config read a *different* field from the items a forward link points at and reduce the collected values. Where aggregation flows *against* links (children roll up to parents) and `compute` stays on one item, a pull follows a link *forward* — one hop — and reduces what it finds there:

```yaml
fields:
  depends_on:
    type: links
    allow_cycles: false

  start:
    type: date
    pull:
      over: depends_on   # follow this link field forward
      field: end         # read this field on each linked item
      function: max      # reduce: start when the last dependency ends

  end:
    type: date
    compute: start + duration
```

| Option | Description |
|--------|-------------|
| `over` | The `link` or `links` field followed forward. Must declare `allow_cycles: false` — pulled values need an acyclic dependency graph to evaluate in. |
| `field` | The field read on each linked item. |
| `function` | The reduction — the same functions as `aggregate`, keyed on the *source* field's type (see the table under [aggregated fields](#aggregated-fields)). The result must fit the declared type: `count` produces `integer`, `average`/`median` of numbers produce `float`. |
| `error_on_missing` | Report an error naming the linked item and field when a linked item lacks the source value, instead of silently leaving the field absent. Default: `false`. |

The example above is forward scheduling from minimal input: set only `depends_on` and `duration` on every item, plus a hand-written `start` on items with no dependencies. `end` computes from `start + duration`, dependents pull their `start` from their dependencies' `end`, and the chain cascades — transitivity emerges from recursion, one hop at a time. `end` is exclusive (a one-day task starting Jan 5 ends Jan 6), so a successor starting at `max(end)` doesn't double-book the last day. For a handover lag, compose through an intermediate field: pull into `earliest_start`, then `start: compute: earliest_start + $constants.handover_lag`.

Pull values behave like every other derived value: never written to files, visible everywhere, and a hand-written frontmatter value always wins — that is what anchors the roots.

Missing inputs are **all-or-nothing**: if any linked item lacks the source field, the pull yields nothing. A partial reduction would be a silent guess — for `max` over dependency ends, a start date that looks plausible and is wrong. An item whose `over` field is empty or absent also yields nothing (it is a root; write its value by hand). Marking the field `required` turns both cases into load-time diagnostics: an unanchored root reports the plain missing field, an item behind an incomplete dependency reports which `item.field` is missing.

Composition mirrors the other mechanisms: `pull` is mutually exclusive with `compute` and `when` on one field, and cannot be combined with `default`. `pull` + `aggregate` means the pull fills *leaf* items of the rollup hierarchy and the aggregate fills everything above — a milestone's `start` is the `min` of its children, and a `depends_on` on the milestone itself does not feed its own start (its children's dependencies already carry the constraints). Dependencies may point at items whose source value is itself aggregated or computed; all derived values share one dependency graph and evaluate in the right order.

Cycles: a dependency loop within the `over` link field is reported by the cycle detector (that is why `allow_cycles: false` is required); items on the loop simply receive no pulled value. A loop that only the *combination* of link fields produces — two pull fields over two different link graphs that are only jointly cyclic — gets its own diagnostic naming the `item.field` chain.

---

## Resources

Resources are named lists of entities defined in `.workdown/resources.yaml`. They provide valid values for work item fields — instead of hardcoding allowed values in the schema, you reference a resource list that can be maintained independently.

The formal structure of `resources.yaml` is defined in `resources.schema.json`.

### Defining resources

Each top-level key in `resources.yaml` is a resource name. The value is an array of entries, each with a required `id` and optional additional fields:

```yaml
people:
  - id: alice
    name: Alice Smith
    email: alice@example.com
  - id: bob
    name: Bob Jones
    email: bob@example.com

teams:
  - id: backend
    name: Backend Team
  - id: frontend
    name: Frontend Team

sprints:
  - id: sprint-1
    name: Sprint 1
    start: 2026-04-01
    end: 2026-04-14
```

The `id` is the value used in work item fields. Other fields (`name`, `email`, `start`, etc.) are freeform metadata — the CLI does not enforce their structure. Resource names must be lowercase with underscores.

### Linking fields to resources

Add `resource: <name>` to a field definition in `schema.yaml`:

```yaml
fields:
  assignee:
    type: string
    required: false
    resource: people

  sprint:
    type: string
    required: false
    resource: sprints

  reviewers:
    type: list
    required: false
    resource: people
```

The `resource` option is valid on `string` and `list` fields. When set, the CLI validates that the field value matches an `id` from the referenced resource section. For `list` fields, every entry in the list must match.

### How resource values are validated

A value that isn't an entry of its section is a **warning**, not an error: the file still saves, `workdown validate` still exits zero, and the value still renders, groups and filters. `resources.yaml` is data that lags reality, and a new hire assigned before anyone edits the file shouldn't fail a CI run. The warning appears the moment you write the value:

```
$ workdown set implement-login assignee justus
✔ implement-login: assignee: alice → justus
! item 'implement-login', field 'assignee': 'justus' is not an entry in resource 'people'
```

Where the value came from doesn't matter — hand-written, stamped from a `default:`, or derived by `compute:`/`when:`. An unset field never warns; a missing *required* field is already its own error.

Two situations switch the per-item check off, each reported once against `schema.yaml` instead of on every item:

| Situation | Severity | Meaning |
|---|---|---|
| The section isn't declared in `resources.yaml` | error | A typo in `schema.yaml` — `resource: peple` |
| The section is empty, or there is no `resources.yaml` | warning | The list isn't filled in yet; nothing to validate against |

A field's `default:` is held to the same standard, also against `schema.yaml`: a literal default outside a populated section is an error (every item `workdown add` creates would carry an unknown value), and a generator default (`$uuid`, `$filename`, `$filename_pretty`) is an error outright — no generator can produce a resource entry.

### Use cases

- **People**: assignees, reviewers, reporters — `resource: people`
- **Teams**: team assignment, ownership — `resource: teams`
- **Sprints/iterations**: time-boxing work items — `resource: sprints`
- **Components/modules**: categorizing by codebase area — define your own
- **Releases/milestones**: targeting versions — define your own

Resources are flexible. The CLI reads each entry's `id` (the value stored on items) and its optional `name` (the label pickers show); every other attribute is yours to use as documentation. The only rule it enforces is the one above — that fields referencing a resource use valid ids.

### Constants

The top-level key `constants` is reserved. Instead of a resource list, it holds named scalar values defined once per project — data that changes over the project's life (a daily rate, a work-hours-per-day convention) and therefore belongs in `resources.yaml`, not in the schema:

```yaml
constants:
  daily_rate:
    type: float
    value: 800
  work_hours_per_day:
    type: duration
    value: "8h"
```

Each constant declares a `type` — one of the scalar field types (`string`, `integer`, `float`, `date`, `duration`, `boolean`) — and a `value`, which is validated against that type when the file loads. Constants are referenced by name from `schema.yaml` (for example from computed-field expressions as `$constants.<name>`); referencing an undeclared constant is a schema error.

---

## Rules

The `rules:` section defines validation constraints that go beyond single-field checks. Use rules when validation depends on multiple fields, related items, or the collection as a whole.

Each rule has this structure:

```yaml
rules:
  - name: rule-name
    description: What this rule checks
    severity: error           # or warning
    match:                    # which items this applies to (optional)
      <field>: <condition>
    require:                  # what must be true (optional)
      <field>: <assertion>
    count:                    # how many items may match (optional)
      max: 5
```

- `name` is required and must be unique (kebab-case).
- `severity` defaults to `error`. Use `warning` for advisory checks that should not fail validation.
- At least one of `require` or `count` must be present.
- Both `require` and `count` can be used together on the same rule.

### Field references

Both `match` and `require` use field references as keys. A field reference is either a plain field name or a dot-notation path that traverses a relationship:

| Reference | Meaning |
|-----------|---------|
| `status` | Field on the current item |
| `parent.status` | Field on the parent item (via `parent` link field) |
| `children.type` | Field on child items (inverse of `parent` link field) |
| `depends_on.status` | Field on dependency targets (via `depends_on` links field) |

Only one level of dot notation is supported. The first segment must be a `link` or `links` field name (or its inverse).

### Conditions (in `match`)

`match` selects which work items the rule applies to. If omitted, the rule applies to all items. When multiple fields are listed, all conditions must be true (AND).

The value type determines the meaning:

| Form | Meaning | Example |
|------|---------|---------|
| Scalar | Equality | `status: in_progress` |
| Array | Membership (one of) | `type: [bug, task]` |
| Object | Explicit operator | `status: { not: backlog }` |

#### Condition operators (object form)

| Operator | Accepts | Description |
|----------|---------|-------------|
| `not` | value or array | Field does not equal this value (or any in the array) |
| `is_set` | boolean | `true`: field has a value. `false`: field is null/absent |
| `all` | condition | Every related item satisfies the condition |
| `any` | condition | At least one related item satisfies the condition |
| `none` | condition | No related item satisfies the condition |

When multiple operators are specified in the same object, all must be satisfied (AND).

The quantifiers (`all`, `any`, `none`) are only valid when the field reference traverses a one-to-many relationship (a `links` field or the inverse of a `link` field). The value inside a quantifier is itself a condition (same rules: scalar, array, or object).

### Assertions (in `require`)

`require` defines what must be true for each matching item. When multiple fields are listed, all assertions must hold (AND).

| Form | Meaning | Example |
|------|---------|---------|
| `"required"` | Field must be set | `assignee: required` |
| `"forbidden"` | Field must not be set | `parent: forbidden` |
| Object | Explicit operator | `priority: { values: [high, critical] }` |

#### Assertion operators (object form)

| Operator | Accepts | Description |
|----------|---------|-------------|
| `required` | boolean | `true`: field must be set |
| `forbidden` | boolean | `true`: field must not be set |
| `values` | array | Field must be one of these values |
| `not` | value or array | Field must not equal this value (or any in the array) |
| `eq_field` | field name or `$today` | Field must equal the referenced field's value |
| `lt_field` | field name or `$today` | Field must be less than the referenced field's value |
| `lte_field` | field name or `$today` | Field must be less than or equal to the referenced field's value |
| `gt_field` | field name or `$today` | Field must be greater than the referenced field's value |
| `gte_field` | field name or `$today` | Field must be greater than or equal to the referenced field's value |
| `min_count` | integer | Related items must number at least this many |
| `max_count` | integer | Related items must number at most this many |

When multiple operators are specified in the same object, all must be satisfied (AND).

Field-to-field comparisons (`eq_field`, `lte_field`, etc.) are skipped when either field is null. A missing value is not a validation error for comparisons — use `required` to enforce presence separately.

The operand may also be `$today` — the evaluation date (ADR-010), so a rule can compare a field against the present:

```yaml
- name: future-start-for-todo
  description: A to_do item must not have a start_date in the past.
  severity: warning
  match:
    status: to_do
  require:
    start_date:
      gte_field: $today
```

A rule using `$today` makes validation depend on the day it runs — the same files can pass on Monday and warn on Tuesday with no edits. That is the point (the calendar moved), but for reproducible runs every evaluating command takes `--as-of <YYYY-MM-DD>`, which pins `$today` for rules and computed fields alike.

### Count (collection-wide)

`count` limits how many items in the entire project may match the rule's `match` condition. Use this for constraints like WIP limits.

| Option | Description |
|--------|-------------|
| `min` | At least this many items must match |
| `max` | At most this many items may match |

At least one of `min` or `max` must be specified.

---

## Examples

### Level 2: Cross-field (same item)

**Require assignee when in progress:**

```yaml
- name: in-progress-needs-assignee
  description: Work items in progress must have an assignee
  match:
    status: in_progress
  require:
    assignee: required
```

**Bugs must have a priority:**

```yaml
- name: bugs-need-priority
  match:
    type: bug
  require:
    priority: required
```

**Start date must be before end date:**

```yaml
- name: dates-ordered
  description: Start date cannot be after end date
  require:
    start_date:
      lte_field: end_date
```

No `match` — this applies to all items. The comparison is skipped if either date is null.

**Closed items need a resolution:**

```yaml
- name: closed-needs-resolution
  match:
    status: closed
  require:
    resolution:
      required: true
      values: [fixed, wontfix, duplicate]
```

Multiple assertion operators on the same field: both `required` and `values` must hold (AND).

### Level 3: Relationship-based

**Parent cannot be in backlog if child is active:**

```yaml
- name: parent-not-backlog-when-child-active
  match:
    status: in_progress
  require:
    parent.status:
      not: backlog
```

For every item whose status is `in_progress`, the parent's status must not be `backlog`.

**Epic children must be tasks or bugs:**

```yaml
- name: epic-children-types
  match:
    type: epic
  require:
    children.type:
      values: [task, bug]
```

**Warn when all children are closed but parent is not:**

```yaml
- name: close-parent-when-children-done
  severity: warning
  match:
    children.status:
      all: closed
  require:
    status: closed
```

The `all` quantifier in `match`: this rule applies to items where every child has `status: closed`. The `require` then checks the item itself.

**Every epic must have at least one child:**

```yaml
- name: epics-need-children
  match:
    type: epic
  require:
    children:
      min_count: 1
```

### Level 4: Collection-wide

**WIP limit — at most 5 items in progress:**

```yaml
- name: wip-limit
  description: At most 5 items in progress at once
  match:
    status: in_progress
  count:
    max: 5
```

**WIP limit as a warning instead of error:**

```yaml
- name: wip-limit
  severity: warning
  match:
    status: in_progress
  count:
    max: 5
```

**Combined require and count — active items need assignees, max 5 total:**

```yaml
- name: wip-limit-with-assignee
  match:
    status: in_progress
  require:
    assignee: required
  count:
    max: 5
```

Both `require` and `count` on the same rule. Each matching item must have an assignee, and the total count of matching items must not exceed 5.

---

## Null handling

Rules interact with null (absent) fields as follows:

- **Conditions in `match`:** A condition on a null field evaluates to false — the item does not match. Exception: `{ is_set: false }` explicitly matches null fields.
- **Assertions in `require`:** The `required` assertion on a null field is a violation (that is its purpose). Field-to-field comparisons (`eq_field`, `lte_field`, etc.) are skipped when either operand is null — null is undefined, not a validation error.
- **Relationship traversal:** Traversing a link/links field that is null yields no related items. Quantifiers on empty sets follow logic conventions: `all` is vacuously true (there are no items to violate it), `any` is false (there are no items to satisfy it), `none` is true (there are no items to violate it).
