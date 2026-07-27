// Text-on-color contrast: given a surface tinted with an item's color,
// pick black or white text by the color's WCAG relative luminance.
//
// The API only ever delivers resolved `#rrggbb` hex (core resolves
// palette names server-side), so that is the only input shape handled
// here. The chosen color is absolute data — the same in light and dark
// mode — so the text decision is per-color, never per-theme.

/**
 * The luminance above which black text has more WCAG contrast than
 * white. Derived from the contrast-ratio definition: contrast against
 * white is `1.05 / (L + 0.05)`, against black `(L + 0.05) / 0.05`;
 * they cross at `L = sqrt(0.0525) - 0.05 ≈ 0.179`. A tuning knob for
 * the visual pass — raising it biases toward white text.
 */
const BLACK_TEXT_LUMINANCE_THRESHOLD = 0.179;

/**
 * WCAG relative luminance of a `#rrggbb` color, in `[0, 1]`.
 * Linearizes each sRGB channel, then weights: `0.2126 R + 0.7152 G +
 * 0.0722 B`.
 */
export function relativeLuminance(hex: string): number {
	const [red, green, blue] = hexChannels(hex);
	return 0.2126 * linearize(red) + 0.7152 * linearize(green) + 0.0722 * linearize(blue);
}

/**
 * The readable text color — `#000000` or `#ffffff` — for text sitting
 * on a surface filled with `hex`.
 */
export function textColorOn(hex: string): '#000000' | '#ffffff' {
	return relativeLuminance(hex) > BLACK_TEXT_LUMINANCE_THRESHOLD ? '#000000' : '#ffffff';
}

/** The three sRGB channels of `#rrggbb`, each in `[0, 1]`. */
function hexChannels(hex: string): [number, number, number] {
	const digits = hex.replace('#', '');
	return [
		parseInt(digits.slice(0, 2), 16) / 255,
		parseInt(digits.slice(2, 4), 16) / 255,
		parseInt(digits.slice(4, 6), 16) / 255
	];
}

/** sRGB gamma expansion of one channel. */
function linearize(channel: number): number {
	return channel <= 0.04045 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
}
