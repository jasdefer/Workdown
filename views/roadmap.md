# Gantt

Timeline of items from `start_date` to `end_date`, grouped by `type`.

```mermaid
gantt
    dateFormat YYYY-MM-DD
    section milestone
    Renderers :renderers, 2026-04-24, 2026-05-01
    Item mutations :item-mutations, 2026-05-02, 2026-05-29
    Interactive UI (workdown serve) :server, 2026-05-07, 2026-06-25
    Polish & dogfood :polish, 2026-06-26, 2026-07-02
    section epic
    Phase 04 Visualization :phase-04-visualization, 2026-04-24, 2026-07-02
```

> _4 items dropped:_
> _- missing 'start_date': "Code-quality cleanup", "Foundation", "Multi-project support", "Time tracking"_
