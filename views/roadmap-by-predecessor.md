# Gantt

Timeline of items starting at `max(start_date, predecessor end)` for `duration` each; predecessors from `depends_on`, grouped by `type`.

```mermaid
gantt
    dateFormat YYYY-MM-DD
    section epic
    Phase 04 Visualization :phase-04-visualization, 2026-04-24, 2026-07-02
```

> _8 items dropped:_
> _- no anchor: "Code-quality cleanup", "Foundation", "Time tracking"_
> _- predecessor 'foundation' unresolved: "Item mutations", "Renderers", "Interactive server"_
> _- predecessor 'frontend' unresolved: "Polish & dogfood"_
> _- predecessor 'server' unresolved: "Frontend"_
