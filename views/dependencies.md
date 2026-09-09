# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    subgraph burndown-chart ["A chart that shows progress over time (burndown or similar)"]
        burndown-chart-design["Decide where the burndown's time axis comes from"]
    end
    config-hot-reload["Read config.yaml per request so it hot-reloads like everything else"]
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        typed-date-filter-comparison["Compare dates in filters as dates, not as text"]
    end
    subgraph multi-project-support ["Multi-project support"]
        multi-project-design["Design multi-project support — set decisions and break out follow-up work"]
    end
    prepare-commit-msg-hook["Prefill terminal commits with the generated message"]
    project-load-cache["Cache the project load in the server (when it starts to hurt)"]
    recording-dot-extraction["Extract the recording indicator the six item-presenting views each rebuilt"]
    subgraph schema-editor-web ["See and edit the schema in the web app"]
        schema-editor-web-design["Decide how much of the schema the web app edits, and what a breaking save does"]
    end
    subgraph schema-expressions ["Derived field expressions"]
        compute-type-support-mismatch["Decide which field types may declare compute and pull"]
        expression-comparison-corner-cases["Integer precision and NaN in comparison evaluation"]
        expression-logical-combinators["`and` / `or` / `not` in the expression grammar"]
        when-map-shorthand["`map:` — lookup-table shorthand over the `when:` evaluator"]
        when-then-value-expressions["`then:` values beyond literals — `$today`, fields, expressions"]
    end
    status-transition-dates["Fill in a date when a status changes, instead of typing it by hand"]
```
