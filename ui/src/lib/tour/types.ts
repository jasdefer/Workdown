// Shared shapes for the project tour.
//
// The tour is one set of cards moving through a sequence of layouts. A
// `Layout` positions every card in world space (pixels, origin at the
// stage centre, z toward the viewer); a `Scene` pairs one layout with the
// camera poses, decoration (edges, labels) and caption that go with it.
// Everything here is plain data so the layout and camera math stay
// testable without a DOM.

/** The one card shape every scene renders, whatever view it came from. */
export interface TourCard {
	id: string;
	title: string;
	subtitle: string | null;
	/** Resolved `#rrggbb` from the view's `color` display role, or null. */
	background: string | null;
	/**
	 * Scale guard: above the card-count threshold, leaf items render as
	 * small dots instead of full cards (structure, not details).
	 */
	compact: boolean;
}

export interface Position {
	x: number;
	y: number;
	z: number;
	/** 0 hides a card a scene has no place for (parked behind the stage). */
	opacity: number;
}

export type Layout = Map<string, Position>;

export interface Bounds {
	minX: number;
	maxX: number;
	minY: number;
	maxY: number;
}

/** A text label placed in world space (column headers, time ticks). */
export interface WorldLabel {
	x: number;
	y: number;
	text: string;
	align: 'start' | 'center' | 'end';
	tone: 'muted' | 'accent';
}

/** A connector drawn in world space once the cards have settled. */
export interface WorldEdge {
	from: Position;
	to: Position;
	/** `down`: parent above child (tree). `right`: dependency flows left→right. */
	direction: 'down' | 'right';
	/** Highlighted — the edges into the most depended-upon item. */
	hot: boolean;
}

/** The world container's transform: translate first, then tilt. */
export interface CameraPose {
	tx: number;
	ty: number;
	tz: number;
	/** Degrees. */
	rx: number;
	ry: number;
}

export interface MetricTile {
	label: string;
	value: string;
}

export interface Scene {
	name: string;
	/** One sentence, shown once the cards have settled. */
	caption: string | null;
	/** How long the scene stays after its cards have settled. */
	holdMs: number;
	layout: Layout;
	edges: WorldEdge[];
	labels: WorldLabel[];
	/** `enter` is reached as the cards settle; the camera then eases to `hold`. */
	camera: { enter: CameraPose; hold: CameraPose };
	/** Cards passing the camera fade out instead of blowing up. */
	flythrough: boolean;
	/** Opacity multiplier for every card: below 1 the cards recede to a backdrop (title, numbers). */
	dim: number;
	/** Vertical "today" line at world x = 0. */
	todayLine: boolean;
	overlay: { kind: 'title' } | { kind: 'metrics'; tiles: MetricTile[] } | null;
	/** Set on the last scene: the view the tour opens when it ends. */
	landingViewId: string | null;
}
