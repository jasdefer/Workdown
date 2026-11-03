// Item references in a work item's Markdown body, and what they turn
// into on screen.
//
// Two spellings point at another item: `[[id]]`, the wikilink this
// repository's own items use, and `[text](id.md)`, the relative link
// GitHub renders because every item is a sibling file in one flat
// directory. Both resolve the same way: to the item with that id, if
// it exists. `itemLinkExtension` is the `marked` extension that renders
// the wikilink as a link and rewrites the `.md` link; both go through
// `referenceOfId` / `referenceOfHref` to decide, and the item editor's
// follow row under a link field uses the same functions, so a body link
// and a `parent` value point — and fail — the same way.
//
// Where a link leads is the host's business: in the item dialog it
// swaps the dialog's item (`?item=`), on the standalone page it
// navigates to `/items/:id`. The host says so through `hrefFor`. Which
// ids exist comes from the schema store's item index, `null` while that
// has not loaded; then every reference is taken to exist rather than
// flashing dead until the index arrives.

import type { MarkedExtension, Tokens } from 'marked';

export interface ItemLinkResolver {
	/** Every work item id, or `null` while the index is not known yet. */
	knownIds: ReadonlySet<string> | null;
	/** The href that opens the item `id` the way this host opens items. */
	hrefFor: (id: string) => string;
}

/** Where an id leads: a bare id always names an item, present or not. */
export type ItemTarget =
	/** An item that exists; `href` opens it. */
	| { kind: 'item'; id: string; href: string }
	/** A reference to an id no item has. */
	| { kind: 'missing'; id: string };

/** Where a link's href leads: an item target, or nothing of ours. */
export type ItemReference =
	| ItemTarget
	/** Not an item reference: an external URL, a path, an anchor. */
	| { kind: 'other' };

/** What a bare id — a `[[id]]`, a `parent` value — refers to. */
export function referenceOfId(id: string, resolver: ItemLinkResolver): ItemTarget {
	if (resolver.knownIds !== null && !resolver.knownIds.has(id)) {
		return { kind: 'missing', id };
	}
	return { kind: 'item', id, href: resolver.hrefFor(id) };
}

// `id.md` or `./id.md` and nothing else: a sibling file in the items
// directory is an item by construction (the directory is flat). A path
// with a directory part, a scheme, a query or a fragment is not ours.
const SIBLING_MARKDOWN = /^(?:\.\/)?([^/\\:?#]+)\.md$/;

/** What a Markdown link's href refers to. */
export function referenceOfHref(href: string, resolver: ItemLinkResolver): ItemReference {
	const id = SIBLING_MARKDOWN.exec(href)?.[1];
	if (id === undefined) return { kind: 'other' };
	return referenceOfId(id, resolver);
}

interface WikilinkToken extends Tokens.Generic {
	type: 'wikilink';
	id: string;
}

const WIKILINK = /^\[\[([^[\]\n]+)\]\]/;

/**
 * The `marked` extension: `[[id]]` becomes a link, and a `[text](id.md)`
 * link is pointed at the item instead of at a relative path that 404s.
 * A reference to an id no item has is rendered as inert text that says
 * so in its title, so it reads differently from a working link.
 */
export function itemLinkExtension(resolver: ItemLinkResolver): MarkedExtension {
	return {
		extensions: [
			{
				name: 'wikilink',
				level: 'inline',
				start: (src) => src.indexOf('[['),
				tokenizer(src): WikilinkToken | undefined {
					const match = WIKILINK.exec(src);
					const id = match?.[1]?.trim();
					if (match === null || id === undefined || id.length === 0) return undefined;
					return { type: 'wikilink', raw: match[0], id };
				},
				renderer(token) {
					const { id } = token as WikilinkToken;
					return referenceHtml(referenceOfId(id, resolver), escapeHtml(id), null);
				}
			}
		],
		renderer: {
			link(token) {
				const reference = referenceOfHref(token.href, resolver);
				if (reference.kind === 'other') return false;
				return referenceHtml(reference, this.parser.parseInline(token.tokens), token.title);
			}
		}
	};
}

// `label` is HTML already: escaped text, or parsed inline tokens.
function referenceHtml(
	reference: ItemTarget,
	label: string,
	title: string | null | undefined
): string {
	if (reference.kind === 'missing') {
		const explanation = escapeHtml(missingTitle(reference.id));
		return `<span class="item-link dead" title="${explanation}">${label}</span>`;
	}
	const titleAttr = title ? ` title="${escapeHtml(title)}"` : '';
	return `<a class="item-link" href="${escapeHtml(reference.href)}"${titleAttr}>${label}</a>`;
}

/** The explanation a dead reference carries, in the body and the follow row alike. */
export function missingTitle(id: string): string {
	return `No item '${id}'`;
}

function escapeHtml(text: string): string {
	return text
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&#39;');
}
