# Table

| id | type | status | parent | depends_on |
| --- | --- | --- | --- | --- |
| [adr-phase-04-architecture](../workdown-items/adr-phase-04-architecture.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [aggregate-rollup](../workdown-items/aggregate-rollup.md) | issue | done | [renderers](../workdown-items/renderers.md) |  |
| [cli-add-audit](../workdown-items/cli-add-audit.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-body-command](../workdown-items/cli-body-command.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-move-command](../workdown-items/cli-move-command.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [cli-rename-command](../workdown-items/cli-rename-command.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-set-command](../workdown-items/cli-set-command.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-set-modes](../workdown-items/cli-set-modes.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [cli-unset-command](../workdown-items/cli-unset-command.md) | issue | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [code-quality](../workdown-items/code-quality.md) | milestone | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) |  |
| [cross-cutting-helpers](../workdown-items/cross-cutting-helpers.md) | issue | done | [code-quality](../workdown-items/code-quality.md) |  |
| [diagnostic-scope-routing](../workdown-items/diagnostic-scope-routing.md) | issue | done | [code-quality](../workdown-items/code-quality.md) |  |
| [diagnostic-variant-cleanup](../workdown-items/diagnostic-variant-cleanup.md) | issue | done | [code-quality](../workdown-items/code-quality.md) |  |
| [duration-comparison-rule](../workdown-items/duration-comparison-rule.md) | issue | to_do | [time-tracking](../workdown-items/time-tracking.md) |  |
| [duration-field-type](../workdown-items/duration-field-type.md) | issue | done | [renderers](../workdown-items/renderers.md) |  |
| [field-value-native-date](../workdown-items/field-value-native-date.md) | issue | done | [renderers](../workdown-items/renderers.md) |  |
| [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) | issue | to_do | [server](../workdown-items/server.md) | [walking-skeleton](../workdown-items/walking-skeleton.md) |
| [foundation](../workdown-items/foundation.md) | milestone | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) |  |
| [foundation-cleanup](../workdown-items/foundation-cleanup.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [gantt-duration-mode](../workdown-items/gantt-duration-mode.md) | issue | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md), [duration-field-type](../workdown-items/duration-field-type.md) |
| [gantt-predecessor-mode](../workdown-items/gantt-predecessor-mode.md) | issue | done | [renderers](../workdown-items/renderers.md) | [gantt-duration-mode](../workdown-items/gantt-duration-mode.md) |
| [git-derived-default-generator](../workdown-items/git-derived-default-generator.md) | issue | to_do | [time-tracking](../workdown-items/time-tracking.md) |  |
| [item-mutations](../workdown-items/item-mutations.md) | milestone | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md) |
| [live-updates](../workdown-items/live-updates.md) | issue | to_do | [server](../workdown-items/server.md) | [walking-skeleton](../workdown-items/walking-skeleton.md) |
| [multi-project-design](../workdown-items/multi-project-design.md) | issue | to_do | [multi-project-support](../workdown-items/multi-project-support.md) |  |
| [multi-project-support](../workdown-items/multi-project-support.md) | epic | to_do |  |  |
| [mutations-slice](../workdown-items/mutations-slice.md) | issue | to_do | [server](../workdown-items/server.md) | [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) |
| [phase-04-visualization](../workdown-items/phase-04-visualization.md) | epic | in_progress |  |  |
| [polish](../workdown-items/polish.md) | milestone | to_do | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [frontend](../workdown-items/frontend.md) |
| [remaining-read-views](../workdown-items/remaining-read-views.md) | issue | to_do | [server](../workdown-items/server.md) | [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) |
| [render-bar-chart](../workdown-items/render-bar-chart.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-board](../workdown-items/render-board.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-command](../workdown-items/render-command.md) | issue | done | [renderers](../workdown-items/renderers.md) | [render-board](../workdown-items/render-board.md), [render-tree](../workdown-items/render-tree.md), [render-graph](../workdown-items/render-graph.md), [render-table](../workdown-items/render-table.md), [render-gantt](../workdown-items/render-gantt.md), [render-bar-chart](../workdown-items/render-bar-chart.md), [render-line-chart](../workdown-items/render-line-chart.md), [render-workload](../workdown-items/render-workload.md), [render-metric](../workdown-items/render-metric.md), [render-treemap](../workdown-items/render-treemap.md), [render-heatmap](../workdown-items/render-heatmap.md) |
| [render-gantt](../workdown-items/render-gantt.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-gantt-by-depth](../workdown-items/render-gantt-by-depth.md) | issue | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md) |
| [render-gantt-by-initiative](../workdown-items/render-gantt-by-initiative.md) | issue | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md) |
| [render-graph](../workdown-items/render-graph.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-heatmap](../workdown-items/render-heatmap.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-line-chart](../workdown-items/render-line-chart.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-metric](../workdown-items/render-metric.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-module-hygiene](../workdown-items/render-module-hygiene.md) | issue | done | [code-quality](../workdown-items/code-quality.md) |  |
| [render-table](../workdown-items/render-table.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-tree](../workdown-items/render-tree.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-treemap](../workdown-items/render-treemap.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-workload](../workdown-items/render-workload.md) | issue | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [renderers](../workdown-items/renderers.md) | milestone | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md) |
| [rules-current-date-reference](../workdown-items/rules-current-date-reference.md) | issue | to_do | [code-quality](../workdown-items/code-quality.md) |  |
| [server](../workdown-items/server.md) | milestone | to_do | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md), [item-mutations](../workdown-items/item-mutations.md), [renderers](../workdown-items/renderers.md) |
| [store-diagnostics-consistency](../workdown-items/store-diagnostics-consistency.md) | issue | to_do | [polish](../workdown-items/polish.md) |  |
| [time-tracking](../workdown-items/time-tracking.md) | milestone | to_do |  |  |
| [view-data-intermediate](../workdown-items/view-data-intermediate.md) | issue | done | [renderers](../workdown-items/renderers.md) | [field-value-native-date](../workdown-items/field-value-native-date.md), [views-title-slot](../workdown-items/views-title-slot.md) |
| [views-config-path](../workdown-items/views-config-path.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [views-cross-file-validation](../workdown-items/views-cross-file-validation.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [views-json-schema](../workdown-items/views-json-schema.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [views-title-slot](../workdown-items/views-title-slot.md) | issue | done | [renderers](../workdown-items/renderers.md) |  |
| [views-validate-integration](../workdown-items/views-validate-integration.md) | issue | done | [foundation](../workdown-items/foundation.md) | [views-config-path](../workdown-items/views-config-path.md), [views-cross-file-validation](../workdown-items/views-cross-file-validation.md), [foundation-cleanup](../workdown-items/foundation-cleanup.md) |
| [views-yaml-design](../workdown-items/views-yaml-design.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
| [walker-primitives](../workdown-items/walker-primitives.md) | issue | done | [code-quality](../workdown-items/code-quality.md) |  |
| [walking-skeleton](../workdown-items/walking-skeleton.md) | issue | to_do | [server](../workdown-items/server.md) |  |
| [workspace-refactor](../workdown-items/workspace-refactor.md) | issue | done | [foundation](../workdown-items/foundation.md) |  |
