import { describe, it, expect } from 'vitest';
import { relativeLuminance, textColorOn } from './colorContrast';

describe('relativeLuminance', () => {
	it('is 0 for black and 1 for white', () => {
		expect(relativeLuminance('#000000')).toBe(0);
		expect(relativeLuminance('#ffffff')).toBeCloseTo(1, 10);
	});

	it('weights green highest, blue lowest (WCAG coefficients)', () => {
		expect(relativeLuminance('#00ff00')).toBeCloseTo(0.7152, 4);
		expect(relativeLuminance('#ff0000')).toBeCloseTo(0.2126, 4);
		expect(relativeLuminance('#0000ff')).toBeCloseTo(0.0722, 4);
	});

	it('handles uppercase hex digits', () => {
		expect(relativeLuminance('#FFFFFF')).toBeCloseTo(1, 10);
	});
});

describe('textColorOn', () => {
	it('puts white text on dark surfaces', () => {
		expect(textColorOn('#000000')).toBe('#ffffff');
		expect(textColorOn('#0000ff')).toBe('#ffffff');
		// Dark gray, well below the crossover.
		expect(textColorOn('#333333')).toBe('#ffffff');
	});

	it('puts black text on light surfaces', () => {
		expect(textColorOn('#ffffff')).toBe('#000000');
		expect(textColorOn('#eab308')).toBe('#000000'); // palette yellow
		expect(textColorOn('#00ff00')).toBe('#000000');
	});

	it('decides by WCAG contrast at the crossover, not by hue intuition', () => {
		// Pure red sits just above the crossover (L ≈ 0.213): black text
		// has the higher contrast ratio even though white "looks" usual.
		expect(textColorOn('#ff0000')).toBe('#000000');
	});
});
