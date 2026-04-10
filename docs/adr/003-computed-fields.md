# ADR-003: Computed/aggregated fields

**Status:** Accepted
**Date:** 2026-04-10

## Context

Work items form parent-child hierarchies. Certain fields (dates, estimates) should propagate up the tree automatically rather than being maintained manually at every level.

## Decision

Fields can declare an `aggregate` configuration with a function (sum, min, max, average, median, count, all, any, none) and `error_on_missing` behavior. These fields are set manually on leaf items and computed automatically for parent items. If two items in the same ancestor-child chain both define the value manually, it is a validation error.

## Consequences

- Parent items automatically reflect their children's data
- The CLI computes aggregates at validation/query time, not stored in files
- Detail rules (what happens when a leaf becomes a parent) to be defined during implementation
