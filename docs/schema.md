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

### Available types

| Type | Description | Type-specific options |
|------|-------------|---------------------|
| `string` | Free text | `pattern` (regex) |
| `choice` | Pick one from a list | `values` (required) |
| `multichoice` | Pick zero or more from a list | `values` (required) |
| `integer` | Whole number | `min`, `max` |
| `float` | Decimal number | `min`, `max` |
| `date` | Calendar date (YYYY-MM-DD) | |
| `boolean` | True or false | |
| `list` | List of free-text strings | |
| `link` | Single reference to another work item | `allow_cycles` |
| `links` | Multiple references to other work items | `allow_cycles` |

### Common options (all types)

| Option | Type | Description |
|--------|------|-------------|
| `type` | string | Required. One of the types above. |
| `required` | boolean | Whether the field must be present. Default: `false`. |
| `default` | value | Default value applied by `workdown add`. Can be a literal or a generator. |
| `description` | string | Human-readable explanation. |
| `resource` | string | Name of a resource section in `resources.yaml`. Valid for `string` and `list` types. See [Resources](#resources). |
| `aggregate` | object | Aggregation config for computed fields (see below). |

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
| `error_on_missing` | Whether to report an error if a leaf item is missing this field. Default: `false`. |

Available aggregate functions by type:

| Type | Functions |
|------|-----------|
| `integer`, `float` | `sum`, `min`, `max`, `average`, `median`, `count` |
| `date` | `min`, `max` |
| `boolean` | `all`, `any`, `none` |

If two items in the same ancestor chain both define the value manually, it is a validation error.

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

### Use cases

- **People**: assignees, reviewers, reporters — `resource: people`
- **Teams**: team assignment, ownership — `resource: teams`
- **Sprints/iterations**: time-boxing work items — `resource: sprints`
- **Components/modules**: categorizing by codebase area — define your own
- **Releases/milestones**: targeting versions — define your own

Resources are flexible. The CLI only enforces that `id` values are unique within a resource and that fields referencing a resource use valid ids. Everything else is up to you.

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
| `eq_field` | field name | Field must equal the referenced field's value |
| `lt_field` | field name | Field must be less than the referenced field's value |
| `lte_field` | field name | Field must be less than or equal to the referenced field's value |
| `gt_field` | field name | Field must be greater than the referenced field's value |
| `gte_field` | field name | Field must be greater than or equal to the referenced field's value |
| `min_count` | integer | Related items must number at least this many |
| `max_count` | integer | Related items must number at most this many |

When multiple operators are specified in the same object, all must be satisfied (AND).

Field-to-field comparisons (`eq_field`, `lte_field`, etc.) are skipped when either field is null. A missing value is not a validation error for comparisons — use `required` to enforce presence separately.

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
