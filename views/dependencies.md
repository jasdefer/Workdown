# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    subgraph burndown-chart ["A chart that shows progress over time (burndown or similar)"]
        burndown-chart-design["Decide where the burndown's time axis comes from"]
    end
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        prepare-commit-msg-hook["Prefill terminal commits with the generated message"]
        project-load-cache["Cache the project load in the server (when it starts to hurt)"]
        recording-dot-extraction["Extract the recording indicator the six item-presenting views each rebuilt"]
    end
    subgraph multi-project-support ["Multi-project support"]
        multi-project-design["Design multi-project support — set decisions and break out follow-up work"]
    end
    subgraph schema-editor-web ["See and edit the schema in the web app"]
        schema-derived-field-editor["Edit compute, when, pull and aggregate blocks in the field editor"]
        schema-field-editor-relation["Field editor block for link and links"]
        schema-field-rename["Rename a field and everything that names it"]
        schema-field-reorder["Drag fields into a new order on the schema page"]
        schema-field-type-change["Let an existing field change type, but only to a type that keeps every value valid"]
        schema-rules-editor["Edit rules on the schema page"]
        schema-string-to-choice["Turn a free-text field into a choice, with the values it already holds"]
    end
    subgraph schema-expressions ["Derived field expressions"]
        expression-logical-combinators["`and` / `or` / `not` in the expression grammar"]
        when-map-shorthand["`map:` — lookup-table shorthand over the `when:` evaluator"]
        when-then-value-expressions["`then:` values beyond literals — `$today`, fields, expressions"]
    end
    status-transition-dates["Fill in a date when a status changes, instead of typing it by hand"]
    schema-rules-editor --> schema-field-type-change
    schema-string-to-choice --> schema-field-type-change
```
