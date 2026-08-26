# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    subgraph maintenance-review-2026-08 ["Maintenance pass: findings from the 2026-08 codebase review"]
        assorted-small-fixes["Grab bag of small consistency fixes from the review"]
        chart-renderer-sharing["Make the terminal chart renderers share what they each rebuilt"]
        message-style-consistency["One voice for validation messages"]
        metric-row-check-unification["Stop validating views and metric rows with two copies of every check"]
        query-value-consolidation["Deduplicate the filter engine's comparison and formatting logic"]
        render-flow-doc["One page that shows how a render flows through the system"]
        schema-property-table["Table-drive the 'is this property allowed on this field type?' check"]
        stale-docs-refresh["Fix the documentation that is actively wrong"]
        stateful-test-gaps["Test the two stateful areas that currently have no coverage"]
        view-kind-sync-guards["Make the non-Rust view-kind mirrors fail loudly when they drift"]
        web-layer-adr["Write down the web layer's design decisions as an ADR"]
    end
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        tags-view["A view over tags"]
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
    message-style-consistency --> metric-row-check-unification
```
