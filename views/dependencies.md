# Graph: depends_on

Directed graph of items connected through `depends_on`, nested by `parent`.

```mermaid
flowchart TD
    subgraph burndown-chart ["A chart that shows progress over time (burndown or similar)"]
        burndown-chart-design["Decide where the burndown's time axis comes from"]
    end
    subgraph misc-work ["Miscellaneous improvements"]
        evaluation-date-single-read["One clock read per invocation, writes included"]
        item-detail-hide-optional-fields["Let the reader hide optional fields in the item detail"]
        item-detail-side-placement["Let the item detail open at the side again for quick browsing"]
        prepare-commit-msg-hook["Prefill terminal commits with the generated message"]
        project-load-cache["Cache the project load in the server (when it starts to hurt)"]
        recording-dot-extraction["Extract the recording indicator the six item-presenting views each rebuilt"]
        rule-comparison-reference-check["Check comparison references in rules at schema load"]
        ui-toolchain-upgrade["Major upgrades of the web UI's lint and SvelteKit toolchain"]
    end
    subgraph multi-project-support ["Multi-project support"]
        multi-project-design["Design multi-project support — set decisions and break out follow-up work"]
    end
    subgraph schema-expressions ["Derived field expressions"]
        expression-logical-combinators["`and` / `or` / `not` in the expression grammar"]
        pull-copy-over-link["Inherit any field type from a linked parent (`pull` over a `link` copies), and `eq_field` on choice"]
        pull-root-anchor["A project-level start for unanchored roots, without a synthetic milestone item"]
        rule-literal-comparisons["Compare a field with a literal in a rule's match block"]
        when-map-shorthand["`map:` — lookup-table shorthand over the `when:` evaluator"]
        when-then-value-expressions["`then:` values beyond literals — `$today`, fields, expressions"]
    end
    status-transition-dates["Fill in a date when a status changes, instead of typing it by hand"]
```
