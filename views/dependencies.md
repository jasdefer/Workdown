# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    config-hot-reload["Read config.yaml per request so it hot-reloads like everything else"]
    subgraph maintenance-review-2026-08 ["Maintenance pass: findings from the 2026-08 codebase review"]
        stale-docs-refresh["Fix the documentation that is actively wrong"]
        stateful-test-gaps["Test the two stateful areas that currently have no coverage"]
        view-kind-sync-guards["Make the non-Rust schema mirrors fail loudly when they drift"]
    end
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        tags-view["A view over tags"]
        typed-date-filter-comparison["Compare dates in filters as dates, not as text"]
        value-coercion-layering["Move value coercion out of the store to break the parser↔store cycle"]
        views-check-metric-row-dedup["Collapse the metric-row duplicates of the generic view checks"]
    end
    subgraph multi-project-support ["Multi-project support"]
        multi-project-design["Design multi-project support — set decisions and break out follow-up work"]
    end
    project-load-cache["Cache the project load in the server (when it starts to hurt)"]
    subgraph schema-expressions ["Derived field expressions"]
        expression-comparison-corner-cases["Integer precision and NaN in comparison evaluation"]
        expression-logical-combinators["`and` / `or` / `not` in the expression grammar"]
        when-map-shorthand["`map:` — lookup-table shorthand over the `when:` evaluator"]
        when-then-value-expressions["`then:` values beyond literals — `$today`, fields, expressions"]
    end
```
