// Which mark an item carries in a view: the one whose detail is open,
// the one opened most recently before that, or none. The open mark
// wins when both ids name the same item, so an item never shows the
// weaker trace while its detail is on screen.
//
// Pure on purpose: the selection store holds the two ids, every view
// kind asks this for each item it draws, and the test pins the
// precedence without a component.

export type ItemMark = 'open' | 'last' | null;

export function markOf(itemId: string, openId: string | null, lastId: string | null): ItemMark {
	if (itemId === openId) return 'open';
	if (itemId === lastId) return 'last';
	return null;
}
