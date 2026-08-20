# Table

| id | status | parent | depends_on |
| --- | --- | --- | --- |
| [adr-phase-04-architecture](../workdown-items/adr-phase-04-architecture.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [aggregate-rollup](../workdown-items/aggregate-rollup.md) | done | [renderers](../workdown-items/renderers.md) |  |
| [app-shell-navigation](../workdown-items/app-shell-navigation.md) | done | [server](../workdown-items/server.md) | [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) |
| [ci-workspace-coverage](../workdown-items/ci-workspace-coverage.md) | done |  |  |
| [cli-add-audit](../workdown-items/cli-add-audit.md) | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-body-command](../workdown-items/cli-body-command.md) | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-move-command](../workdown-items/cli-move-command.md) | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [cli-rename-command](../workdown-items/cli-rename-command.md) | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-set-command](../workdown-items/cli-set-command.md) | done | [item-mutations](../workdown-items/item-mutations.md) |  |
| [cli-set-modes](../workdown-items/cli-set-modes.md) | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [cli-unset-command](../workdown-items/cli-unset-command.md) | done | [item-mutations](../workdown-items/item-mutations.md) | [cli-set-command](../workdown-items/cli-set-command.md) |
| [code-quality](../workdown-items/code-quality.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) |  |
| [color-display-slot](../workdown-items/color-display-slot.md) | done | [view-presentation](../workdown-items/view-presentation.md) | [color-field-type](../workdown-items/color-field-type.md), [view-display-config](../workdown-items/view-display-config.md) |
| [color-field-type](../workdown-items/color-field-type.md) | done | [view-presentation](../workdown-items/view-presentation.md) | [mutations-slice](../workdown-items/mutations-slice.md) |
| [computed-fields](../workdown-items/computed-fields.md) | done | [time-tracking](../workdown-items/time-tracking.md) | [project-constants](../workdown-items/project-constants.md) |
| [conditional-field-value](../workdown-items/conditional-field-value.md) | done | [polish](../workdown-items/polish.md) | [expression-predicates](../workdown-items/expression-predicates.md), [evaluation-time-now](../workdown-items/evaluation-time-now.md) |
| [config-field-role-validation](../workdown-items/config-field-role-validation.md) | done | [time-tracking](../workdown-items/time-tracking.md) |  |
| [cross-cutting-helpers](../workdown-items/cross-cutting-helpers.md) | done | [code-quality](../workdown-items/code-quality.md) |  |
| [diagnostic-scope-routing](../workdown-items/diagnostic-scope-routing.md) | done | [code-quality](../workdown-items/code-quality.md) |  |
| [diagnostic-variant-cleanup](../workdown-items/diagnostic-variant-cleanup.md) | done | [code-quality](../workdown-items/code-quality.md) |  |
| [display-defaults-validation](../workdown-items/display-defaults-validation.md) | done | [view-presentation](../workdown-items/view-presentation.md) | [view-display-config](../workdown-items/view-display-config.md) |
| [duration-delta-absent-value](../workdown-items/duration-delta-absent-value.md) | done | [time-tracking](../workdown-items/time-tracking.md) |  |
| [duration-field-type](../workdown-items/duration-field-type.md) | done | [renderers](../workdown-items/renderers.md) |  |
| [effort-field-config](../workdown-items/effort-field-config.md) | done | [time-tracking](../workdown-items/time-tracking.md) | [config-field-role-validation](../workdown-items/config-field-role-validation.md) |
| [effort-timer](../workdown-items/effort-timer.md) | to_do | [time-tracking](../workdown-items/time-tracking.md) | [effort-field-config](../workdown-items/effort-field-config.md), [duration-delta-absent-value](../workdown-items/duration-delta-absent-value.md) |
| [evaluation-date-single-read](../workdown-items/evaluation-date-single-read.md) | to_do | [misc-work](../workdown-items/misc-work.md) | [evaluation-time-now](../workdown-items/evaluation-time-now.md) |
| [evaluation-time-now](../workdown-items/evaluation-time-now.md) | done | [polish](../workdown-items/polish.md) |  |
| [explicit-in-operator](../workdown-items/explicit-in-operator.md) | done | [polish](../workdown-items/polish.md) | [view-filter-editor](../workdown-items/view-filter-editor.md) |
| [expression-comparison-corner-cases](../workdown-items/expression-comparison-corner-cases.md) | to_do | [schema-expressions](../workdown-items/schema-expressions.md) | [conditional-field-value](../workdown-items/conditional-field-value.md) |
| [expression-logical-combinators](../workdown-items/expression-logical-combinators.md) | to_do | [schema-expressions](../workdown-items/schema-expressions.md) | [expression-predicates](../workdown-items/expression-predicates.md) |
| [expression-predicates](../workdown-items/expression-predicates.md) | done | [polish](../workdown-items/polish.md) |  |
| [field-value-map](../workdown-items/field-value-map.md) | removed | [schema-expressions](../workdown-items/schema-expressions.md) |  |
| [field-value-native-date](../workdown-items/field-value-native-date.md) | done | [renderers](../workdown-items/renderers.md) |  |
| [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) | done | [server](../workdown-items/server.md) | [walking-skeleton](../workdown-items/walking-skeleton.md), [ui-foundation](../workdown-items/ui-foundation.md) |
| [foundation](../workdown-items/foundation.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) |  |
| [foundation-cleanup](../workdown-items/foundation-cleanup.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [gantt-duration-mode](../workdown-items/gantt-duration-mode.md) | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md), [duration-field-type](../workdown-items/duration-field-type.md) |
| [gantt-predecessor-mode](../workdown-items/gantt-predecessor-mode.md) | done | [renderers](../workdown-items/renderers.md) | [gantt-duration-mode](../workdown-items/gantt-duration-mode.md) |
| [github-render-action](../workdown-items/github-render-action.md) | removed | [polish](../workdown-items/polish.md) |  |
| [init-install-hooks](../workdown-items/init-install-hooks.md) | done | [polish](../workdown-items/polish.md) |  |
| [item-mutations](../workdown-items/item-mutations.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md) |
| [live-updates](../workdown-items/live-updates.md) | done | [server](../workdown-items/server.md) | [walking-skeleton](../workdown-items/walking-skeleton.md) |
| [misc-work](../workdown-items/misc-work.md) | to_do |  |  |
| [multi-project-design](../workdown-items/multi-project-design.md) | to_do | [multi-project-support](../workdown-items/multi-project-support.md) |  |
| [multi-project-support](../workdown-items/multi-project-support.md) | to_do |  |  |
| [mutations-slice](../workdown-items/mutations-slice.md) | done | [server](../workdown-items/server.md) | [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) |
| [phase-04-visualization](../workdown-items/phase-04-visualization.md) | done |  |  |
| [polish](../workdown-items/polish.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [view-authoring](../workdown-items/view-authoring.md) |
| [pomodoro-timer](../workdown-items/pomodoro-timer.md) | to_do | [time-tracking](../workdown-items/time-tracking.md) | [effort-timer](../workdown-items/effort-timer.md) |
| [project-constants](../workdown-items/project-constants.md) | done | [time-tracking](../workdown-items/time-tracking.md) |  |
| [pull-fields](../workdown-items/pull-fields.md) | done | [schema-expressions](../workdown-items/schema-expressions.md) | [computed-fields](../workdown-items/computed-fields.md), [aggregate-rollup](../workdown-items/aggregate-rollup.md) |
| [readme-visualization-update](../workdown-items/readme-visualization-update.md) | done | [polish](../workdown-items/polish.md) |  |
| [remaining-read-views](../workdown-items/remaining-read-views.md) | done | [server](../workdown-items/server.md) | [first-view-end-to-end](../workdown-items/first-view-end-to-end.md) |
| [render-bar-chart](../workdown-items/render-bar-chart.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-board](../workdown-items/render-board.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-command](../workdown-items/render-command.md) | done | [renderers](../workdown-items/renderers.md) | [render-board](../workdown-items/render-board.md), [render-tree](../workdown-items/render-tree.md), [render-graph](../workdown-items/render-graph.md), [render-table](../workdown-items/render-table.md), [render-gantt](../workdown-items/render-gantt.md), [render-bar-chart](../workdown-items/render-bar-chart.md), [render-line-chart](../workdown-items/render-line-chart.md), [render-workload](../workdown-items/render-workload.md), [render-metric](../workdown-items/render-metric.md), [render-treemap](../workdown-items/render-treemap.md), [render-heatmap](../workdown-items/render-heatmap.md) |
| [render-gantt](../workdown-items/render-gantt.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-gantt-by-depth](../workdown-items/render-gantt-by-depth.md) | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md) |
| [render-gantt-by-initiative](../workdown-items/render-gantt-by-initiative.md) | done | [renderers](../workdown-items/renderers.md) | [render-gantt](../workdown-items/render-gantt.md) |
| [render-graph](../workdown-items/render-graph.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-heatmap](../workdown-items/render-heatmap.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-line-chart](../workdown-items/render-line-chart.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-metric](../workdown-items/render-metric.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-module-hygiene](../workdown-items/render-module-hygiene.md) | done | [code-quality](../workdown-items/code-quality.md) |  |
| [render-table](../workdown-items/render-table.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-tree](../workdown-items/render-tree.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-treemap](../workdown-items/render-treemap.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [render-workload](../workdown-items/render-workload.md) | done | [renderers](../workdown-items/renderers.md) | [view-data-intermediate](../workdown-items/view-data-intermediate.md) |
| [renderers](../workdown-items/renderers.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md) |
| [resource-label-display](../workdown-items/resource-label-display.md) | removed | [polish](../workdown-items/polish.md) | [resource-option-lists](../workdown-items/resource-option-lists.md) |
| [resource-option-lists](../workdown-items/resource-option-lists.md) | done | [polish](../workdown-items/polish.md) | [mutations-slice](../workdown-items/mutations-slice.md), [schema-metadata-api](../workdown-items/schema-metadata-api.md) |
| [rules-current-date-reference](../workdown-items/rules-current-date-reference.md) | done | [polish](../workdown-items/polish.md) | [evaluation-time-now](../workdown-items/evaluation-time-now.md) |
| [schema-expressions](../workdown-items/schema-expressions.md) | to_do |  |  |
| [schema-metadata-api](../workdown-items/schema-metadata-api.md) | done | [view-authoring](../workdown-items/view-authoring.md) |  |
| [server](../workdown-items/server.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [foundation](../workdown-items/foundation.md), [item-mutations](../workdown-items/item-mutations.md), [renderers](../workdown-items/renderers.md) |
| [store-diagnostics-consistency](../workdown-items/store-diagnostics-consistency.md) | done | [polish](../workdown-items/polish.md) |  |
| [tags-view](../workdown-items/tags-view.md) | to_do | [misc-work](../workdown-items/misc-work.md) |  |
| [time-tracking](../workdown-items/time-tracking.md) | in_progress |  |  |
| [timer-notifications](../workdown-items/timer-notifications.md) | to_do | [time-tracking](../workdown-items/time-tracking.md) | [pomodoro-timer](../workdown-items/pomodoro-timer.md) |
| [ui-foundation](../workdown-items/ui-foundation.md) | done | [server](../workdown-items/server.md) | [walking-skeleton](../workdown-items/walking-skeleton.md) |
| [value-coercion-layering](../workdown-items/value-coercion-layering.md) | to_do | [misc-work](../workdown-items/misc-work.md) |  |
| [view-authoring](../workdown-items/view-authoring.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [server](../workdown-items/server.md) |
| [view-creation](../workdown-items/view-creation.md) | done | [view-authoring](../workdown-items/view-authoring.md) | [view-write-backend](../workdown-items/view-write-backend.md), [schema-metadata-api](../workdown-items/schema-metadata-api.md), [view-filter-editor](../workdown-items/view-filter-editor.md), [app-shell-navigation](../workdown-items/app-shell-navigation.md) |
| [view-data-intermediate](../workdown-items/view-data-intermediate.md) | done | [renderers](../workdown-items/renderers.md) | [field-value-native-date](../workdown-items/field-value-native-date.md), [views-title-slot](../workdown-items/views-title-slot.md) |
| [view-display-config](../workdown-items/view-display-config.md) | done | [view-presentation](../workdown-items/view-presentation.md) | [remaining-read-views](../workdown-items/remaining-read-views.md) |
| [view-edit-delete](../workdown-items/view-edit-delete.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [view-authoring](../workdown-items/view-authoring.md) |
| [view-filter-editor](../workdown-items/view-filter-editor.md) | done | [view-authoring](../workdown-items/view-authoring.md) | [remaining-read-views](../workdown-items/remaining-read-views.md), [schema-metadata-api](../workdown-items/schema-metadata-api.md), [view-write-backend](../workdown-items/view-write-backend.md) |
| [view-presentation](../workdown-items/view-presentation.md) | done | [phase-04-visualization](../workdown-items/phase-04-visualization.md) | [server](../workdown-items/server.md) |
| [view-write-backend](../workdown-items/view-write-backend.md) | done | [view-authoring](../workdown-items/view-authoring.md) |  |
| [views-check-metric-row-dedup](../workdown-items/views-check-metric-row-dedup.md) | to_do | [misc-work](../workdown-items/misc-work.md) |  |
| [views-config-path](../workdown-items/views-config-path.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [views-cross-file-validation](../workdown-items/views-cross-file-validation.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [views-json-schema](../workdown-items/views-json-schema.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [views-title-slot](../workdown-items/views-title-slot.md) | done | [renderers](../workdown-items/renderers.md) |  |
| [views-validate-integration](../workdown-items/views-validate-integration.md) | done | [foundation](../workdown-items/foundation.md) | [views-config-path](../workdown-items/views-config-path.md), [views-cross-file-validation](../workdown-items/views-cross-file-validation.md), [foundation-cleanup](../workdown-items/foundation-cleanup.md) |
| [views-yaml-design](../workdown-items/views-yaml-design.md) | done | [foundation](../workdown-items/foundation.md) |  |
| [virtual-id-in-query-eval](../workdown-items/virtual-id-in-query-eval.md) | done | [polish](../workdown-items/polish.md) |  |
| [virtual-id-in-structural-slots](../workdown-items/virtual-id-in-structural-slots.md) | done | [polish](../workdown-items/polish.md) |  |
| [walker-primitives](../workdown-items/walker-primitives.md) | done | [code-quality](../workdown-items/code-quality.md) |  |
| [walking-skeleton](../workdown-items/walking-skeleton.md) | done | [server](../workdown-items/server.md) |  |
| [when-map-shorthand](../workdown-items/when-map-shorthand.md) | to_do | [schema-expressions](../workdown-items/schema-expressions.md) | [conditional-field-value](../workdown-items/conditional-field-value.md) |
| [when-then-value-expressions](../workdown-items/when-then-value-expressions.md) | to_do | [schema-expressions](../workdown-items/schema-expressions.md) | [conditional-field-value](../workdown-items/conditional-field-value.md) |
| [where-clause-value-validation](../workdown-items/where-clause-value-validation.md) | done | [polish](../workdown-items/polish.md) | [explicit-in-operator](../workdown-items/explicit-in-operator.md) |
| [workspace-refactor](../workdown-items/workspace-refactor.md) | done | [foundation](../workdown-items/foundation.md) |  |
