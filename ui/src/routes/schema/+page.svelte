<!--
  The schema page: what the project's work items are made of, read from
  `GET /api/schema/definition` and shown as the file has it. Two
  sections in file order — Fields, a table with one row per field, and
  Rules, one entry per rule. Nothing is clickable yet; the field editor
  is a later item and mounts as a child of this page.

  The frontend learns nothing about types here. Every badge, summary
  line and default comes from the payload; the type system stays in
  Rust. Rendering of a literal default is delegated to the table view's
  `Cell`, so a choice default is a chip and a boolean a check mark, the
  same as in tables.

  When the schema does not load, the page keeps its heading and shows
  the file path and the parser's message in place of both sections.
  There is no in-browser repair: a file that does not parse has no
  structure to put in a form. Fix it in a text editor; the watcher ping
  refreshes the page.
-->
<script lang="ts">
	import type { PageData } from './$types';
	import Cell from '$lib/views/table/Cell.svelte';
	import {
		fillMechanisms,
		firstLine,
		idFirst,
		loadFailure,
		settingsLine
	} from '$lib/schema/schemaPage';

	let { data }: { data: PageData } = $props();

	const definition = $derived(data.result.data ?? null);
	const fields = $derived(definition === null ? [] : idFirst(definition.fields));
	const failure = $derived(definition === null ? loadFailure(data.result) : null);
</script>

<div class="schema-page">
	<div class="content">
		<h1>Schema</h1>

		{#if failure !== null}
			<section class="broken" aria-label="Schema load failure">
				<p class="broken-lead">
					{#if failure.path !== null}
						<code>{failure.path}</code> could not be loaded.
					{:else}
						The schema could not be loaded.
					{/if}
				</p>
				<pre class="broken-detail">{failure.detail}</pre>
				<p class="muted">Fix the file in a text editor; this page refreshes when it changes.</p>
			</section>
		{:else if definition !== null}
			<section aria-labelledby="fields-heading">
				<h2 id="fields-heading">Fields</h2>
				<div class="table-wrap">
					<table>
						<!-- Fixed layout: the columns share the box's width by these
					     proportions and the description takes the rest, so the
					     table never grows past its container and the ellipsis
					     cut happens inside the available space. -->
						<colgroup>
							<col class="col-name" />
							<col class="col-type" />
							<col class="col-filled" />
							<col class="col-required" />
							<col class="col-default" />
							<col class="col-description" />
						</colgroup>
						<thead>
							<tr>
								<th scope="col">Field</th>
								<th scope="col">Type</th>
								<th scope="col">Filled by</th>
								<th scope="col">Required</th>
								<th scope="col">Default</th>
								<th scope="col">Description</th>
							</tr>
						</thead>
						<tbody>
							{#each fields as field (field.name)}
								{@const summary = settingsLine(field)}
								{@const description = firstLine(field.description)}
								<tr>
									<td class="name-cell">
										<code class="field-name">{field.name}</code>
									</td>
									<td class="type-cell">
										<span>{field.field_type}</span>
										{#if summary !== null}
											<span class="summary">{summary}</span>
										{/if}
									</td>
									<td class="filled-cell">
										{#each fillMechanisms(field) as mechanism (mechanism)}
											<span class="mechanism">{mechanism}</span>
										{/each}
									</td>
									<td class="required-cell">
										{#if field.required}<span aria-label="required">✓</span>{/if}
									</td>
									<td class="default-cell">
										{#if field.default === null}
											{''}
										{:else if field.default.kind === 'generator'}
											<code class="generator">{field.default.generator}</code>
										{:else if field.default.kind === 'literal'}
											<Cell value={field.default.value} fieldType={field.field_type} items={{}} />
										{:else}
											<span class="invalid-default" title={field.default.reason}>
												<span aria-hidden="true">⚠</span>
												<span class="sr-only">invalid default:</span>
												{field.default.text}
											</span>
										{/if}
									</td>
									<td class="description-cell" title={field.description}>
										{description ?? ''}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</section>

			<section aria-labelledby="rules-heading">
				<h2 id="rules-heading">Rules</h2>
				{#if definition.rules.length === 0}
					<p class="muted">No rules defined.</p>
				{:else}
					<ul class="rules">
						{#each definition.rules as rule (rule.name)}
							<li class="rule">
								<div class="rule-header">
									<code class="rule-name">{rule.name}</code>
									<span
										class="severity"
										class:error={rule.severity === 'error'}
										title={rule.severity === 'error'
											? 'An item breaking this rule fails validation'
											: 'An item breaking this rule only gets a warning'}
									>
										{rule.severity}
									</span>
								</div>
								{#if rule.description !== null}
									<p class="rule-description">{rule.description}</p>
								{/if}
								{#if rule.body !== ''}
									<pre class="rule-body">{rule.body}</pre>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			</section>
		{/if}
	</div>
</div>

<style>
	/* The layout's main area is a clipped flex column so view pages can
	   scroll their own regions; this page has one long document and
	   scrolls as a whole. The scroll box spans the full width so its
	   scrollbar sits at the window's edge; the content inside it is
	   capped and centered. */
	.schema-page {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}

	.content {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		width: 100%;
		max-width: 72rem;
		margin: 0 auto;
	}

	h1 {
		font-size: var(--text-lg);
		font-weight: 600;
		margin: 0;
	}

	h2 {
		font-size: var(--text-base);
		font-weight: 600;
		margin: 0 0 var(--space-3);
	}

	.muted {
		color: var(--color-fg-muted);
		margin: 0;
	}

	/* ── Broken state ─────────────────────────────────────────────── */

	.broken {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-error-bg);
		color: var(--color-error-fg);
		max-width: 48rem;
	}

	.broken-lead {
		margin: 0;
		font-weight: 600;
	}

	.broken code {
		background-color: transparent;
		padding: 0;
		font-weight: 600;
	}

	.broken-detail {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-size: var(--text-sm);
	}

	.broken .muted {
		color: inherit;
		opacity: 0.8;
		font-size: var(--text-sm);
	}

	/* ── Fields table ─────────────────────────────────────────────── */

	.table-wrap {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-bg);
	}

	table {
		border-collapse: separate;
		border-spacing: 0;
		width: 100%;
		table-layout: fixed;
	}

	.col-name {
		width: 18%;
	}

	.col-type {
		width: 24%;
	}

	.col-filled {
		width: 11%;
	}

	.col-required {
		width: 5.5rem;
	}

	.col-default {
		width: 14%;
	}

	/* `.col-description` takes what is left. */

	th,
	td {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: top;
		border-bottom: 1px solid var(--color-border);
		overflow: hidden;
		text-overflow: ellipsis;
	}

	thead th {
		background-color: var(--color-surface);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--color-fg-muted);
	}

	tbody tr:last-child td {
		border-bottom: none;
	}

	.name-cell {
		white-space: nowrap;
	}

	.field-name {
		background-color: transparent;
		padding: 0;
	}

	/* "Filled by": blank for a field written by hand, so the few derived
	   fields stand out when scanning the column. */
	.mechanism {
		display: inline-block;
		margin: 0 var(--space-1) var(--space-1) 0;
		padding: 0 var(--space-1);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		font-size: 0.7rem;
		line-height: 1.5;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-fg-muted);
	}

	/* The settings line wraps under the type rather than being cut: it
	   is the information people open the file for, and a row grows only
	   for the few fields with a long value list. One row per field is
	   kept, so the later click-to-edit stays a plain row click. */
	.summary {
		display: block;
		margin-top: 0.125rem;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
		white-space: normal;
		overflow-wrap: anywhere;
	}

	.required-cell {
		text-align: center;
	}

	.generator {
		background-color: transparent;
		padding: 0;
		font-size: var(--text-sm);
	}

	.invalid-default {
		color: var(--color-warning-fg);
		cursor: help;
	}

	/* Cut at the column's width, full text on hover via `title` — the
	   item panel's habit, where the description is a hover on the field
	   name. */
	.description-cell {
		white-space: nowrap;
		color: var(--color-fg-muted);
	}

	/* ── Rules ────────────────────────────────────────────────────── */

	.rules {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.rule {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background-color: var(--color-surface);
	}

	.rule-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.rule-name {
		background-color: transparent;
		padding: 0;
		font-weight: 600;
	}

	.severity {
		padding: 0 var(--space-2);
		border-radius: var(--radius-full);
		font-size: 0.7rem;
		line-height: 1.6;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--color-warning-fg);
		background-color: var(--color-warning-bg);
	}

	.severity.error {
		color: var(--color-error-fg);
		background-color: var(--color-error-bg);
	}

	.rule-description {
		margin: 0;
	}

	.rule-body {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-sm);
		background-color: var(--color-bg);
		font-size: var(--text-sm);
		overflow-x: auto;
	}
</style>
