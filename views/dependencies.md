# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        tags-view["A view over tags"]
        value-coercion-layering["Move value coercion out of the store to break the parser↔store cycle"]
        views-check-metric-row-dedup["Collapse the metric-row duplicates of the generic view checks"]
    end
    subgraph multi-project-support ["Multi-project support"]
        multi-project-design["Design multi-project support — set decisions and break out follow-up work"]
    end
    subgraph phase-04-visualization ["Phase 04: Visualization"]
        view-edit-delete["Edit and delete views from the UI"]
    end
    subgraph schema-expressions ["Derived field expressions"]
        expression-comparison-corner-cases["Integer precision and NaN in comparison evaluation"]
        expression-logical-combinators["`and` / `or` / `not` in the expression grammar"]
        when-map-shorthand["`map:` — lookup-table shorthand over the `when:` evaluator"]
        when-then-value-expressions["`then:` values beyond literals — `$today`, fields, expressions"]
    end
    subgraph time-tracking ["Time tracking"]
        duration-comparison-rule["Cross-field comparison rule for duration values"]
        git-derived-default-generator["Default generator that reads dates from git history"]
    end
```
